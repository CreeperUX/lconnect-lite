//! 按 CPU 厂商选 PawnIO 模块，读封装温度和功耗。
//!
//! - Intel：温度 = TjMax（MSR 0x1A2 [23:16]）- 封装温度读数（MSR 0x1B1 [22:16]）；
//!   功耗 = RAPL 封装能量计数器（MSR 0x611）的增量 × 能量单位（MSR 0x606 [12:8]）/ 时间。
//! - AMD Zen（family 17h-1Ah）：温度 = SMN 0x59800 的 Tctl（[31:21] × 0.125，[19] 置位时再减 49）；
//!   功耗同 RAPL，寄存器是 0xC0010299（单位）和 0xC001029B（封装能量）。

use crate::pawnio::PawnIo;
use anyhow::{bail, Result};
use lcl_core::sensors::CpuSensors;
use std::time::Instant;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

const INTEL_MSR: &[u8] = include_bytes!("../../../vendor/pawnio/IntelMSR.bin");
const AMD_FAMILY_17: &[u8] = include_bytes!("../../../vendor/pawnio/AMDFamily17.bin");

const MSR_TEMPERATURE_TARGET: u32 = 0x1A2;
const MSR_PACKAGE_THERM_STATUS: u32 = 0x1B1;
const MSR_RAPL_POWER_UNIT: u32 = 0x606;
const MSR_PKG_ENERGY_STATUS: u32 = 0x611;
const MSR_AMD_PWR_UNIT: u32 = 0xC001_0299;
const MSR_AMD_PKG_ENERGY: u32 = 0xC001_029B;
const SMN_THM_TCON_CUR_TMP: u64 = 0x0005_9800;

enum Vendor {
    Intel,
    Amd,
}

pub struct Cpu {
    io: PawnIo,
    vendor: Vendor,
    /// TjMax（Intel）。
    tj_max: f32,
    /// 每个能量计数单位多少焦耳。
    energy_unit: f64,
    last_energy: Option<(u32, Instant)>,
    /// 其他监控软件读 PCI 配置空间时也拿这个互斥量（LibreHardwareMonitor 的约定）。
    pci_mutex: HANDLE,
}

// SAFETY: 互斥量句柄可以跨线程用；Cpu 只在读数线程里用。
unsafe impl Send for Cpu {}

impl Cpu {
    pub fn open() -> Result<Self> {
        let (vendor, family) = cpu_id();
        let (io, vendor) = match vendor {
            v if v == *b"GenuineIntel" => (PawnIo::load(INTEL_MSR)?, Vendor::Intel),
            v if v == *b"AuthenticAMD" && (0x17..=0x1A).contains(&family) => (PawnIo::load(AMD_FAMILY_17)?, Vendor::Amd),
            v => bail!("不支持这颗 CPU（{}，family {family:#x}）", String::from_utf8_lossy(&v)),
        };
        let unit_msr = match vendor {
            Vendor::Intel => MSR_RAPL_POWER_UNIT,
            Vendor::Amd => MSR_AMD_PWR_UNIT,
        };
        let esu = (io.read_msr(unit_msr)? >> 8) & 0x1F;
        let tj_max = match vendor {
            Vendor::Intel => match ((io.read_msr(MSR_TEMPERATURE_TARGET)? >> 16) & 0xFF) as f32 {
                t if t > 0.0 => t,
                _ => 100.0,
            },
            Vendor::Amd => 0.0,
        };
        let name: Vec<u16> = "Global\\Access_PCI".encode_utf16().chain([0]).collect();
        // SAFETY: 名字以 0 结尾；失败时返回空句柄，读 SMN 时就不加锁。
        let pci_mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        Ok(Self { io, vendor, tj_max, energy_unit: 0.5f64.powi(esu as i32), last_energy: None, pci_mutex })
    }

    pub fn source(&self) -> &'static str {
        match self.vendor {
            Vendor::Intel => "IntelMSR",
            Vendor::Amd => "AMDFamily17",
        }
    }

    /// 读一次。功耗要两次读数才算得出来，第一次是 None。
    pub fn sample(&mut self) -> CpuSensors {
        let mut out = CpuSensors { source: self.source().into(), ..Default::default() };
        match self.temperature() {
            Ok(t) => out.temp_c = Some(t),
            Err(e) => out.error = Some(format!("{e:#}")),
        }
        match self.power() {
            Ok(p) => out.power_w = p,
            Err(e) => out.error = Some(format!("{e:#}")),
        }
        out
    }

    fn temperature(&self) -> Result<f32> {
        match self.vendor {
            Vendor::Intel => {
                let readout = ((self.io.read_msr(MSR_PACKAGE_THERM_STATUS)? >> 16) & 0x7F) as f32;
                Ok(self.tj_max - readout)
            }
            Vendor::Amd => {
                let v = self.with_pci_lock(|| self.io.execute("ioctl_read_smn", &[SMN_THM_TCON_CUR_TMP], 1))?;
                let raw = *v.first().ok_or_else(|| anyhow::anyhow!("读 Tctl 没有返回值"))? as u32;
                let mut t = (raw >> 21) as f32 * 0.125;
                if raw & (1 << 19) != 0 {
                    t -= 49.0;
                }
                Ok(t)
            }
        }
    }

    fn power(&mut self) -> Result<Option<f32>> {
        let msr = match self.vendor {
            Vendor::Intel => MSR_PKG_ENERGY_STATUS,
            Vendor::Amd => MSR_AMD_PKG_ENERGY,
        };
        let energy = self.io.read_msr(msr)? as u32;
        let now = Instant::now();
        let power = self.last_energy.and_then(|(prev, at)| {
            let dt = now.duration_since(at).as_secs_f64();
            // 32 位计数器会回绕，wrapping_sub 正好处理一次回绕。
            (dt > 0.05).then(|| (f64::from(energy.wrapping_sub(prev)) * self.energy_unit / dt) as f32)
        });
        self.last_energy = Some((energy, now));
        Ok(power)
    }

    fn with_pci_lock<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        if self.pci_mutex.is_null() {
            return f();
        }
        // SAFETY: 句柄有效；最多等 100 ms，拿不到就跳过这一轮。
        let wait = unsafe { WaitForSingleObject(self.pci_mutex, 100) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            bail!("PCI 访问锁被其他程序占着");
        }
        let r = f();
        // SAFETY: 上面拿到了锁。
        unsafe { ReleaseMutex(self.pci_mutex) };
        r
    }
}

impl Drop for Cpu {
    fn drop(&mut self) {
        if !self.pci_mutex.is_null() {
            // SAFETY: 句柄由 CreateMutexW 创建，只关一次。
            unsafe { CloseHandle(self.pci_mutex) };
        }
    }
}

/// （厂商字符串，family）。family 按 CPUID 的规则把扩展字段加进来。
fn cpu_id() -> ([u8; 12], u32) {
    let (l0, l1) = (std::arch::x86_64::__cpuid(0), std::arch::x86_64::__cpuid(1));
    let mut vendor = [0u8; 12];
    vendor[..4].copy_from_slice(&l0.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&l0.edx.to_le_bytes());
    vendor[8..].copy_from_slice(&l0.ecx.to_le_bytes());
    let base = (l1.eax >> 8) & 0xF;
    let family = if base == 0xF { base + ((l1.eax >> 20) & 0xFF) } else { base };
    (vendor, family)
}
