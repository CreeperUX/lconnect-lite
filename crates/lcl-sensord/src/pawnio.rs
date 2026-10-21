//! PawnIO 驱动：打开设备、加载模块、调用模块里的函数。接口按 LibreHardwareMonitor 的 PawnIo.cs。
//! 只走设备 IOCTL 接口（PawnIO 的 GPL 例外条款覆盖这种用法）；模块是 PawnIO.Modules 的编译产物（LGPL-2.1）。

use anyhow::{bail, Result};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;

const DEVICE: &str = r"\\?\GLOBALROOT\Device\PawnIO";
const DEVICE_TYPE: u32 = 41394 << 16;
const IOCTL_LOAD_BINARY: u32 = DEVICE_TYPE | (0x821 << 2);
const IOCTL_EXECUTE_FN: u32 = DEVICE_TYPE | (0x841 << 2);
/// 函数名字段固定 32 字节（ASCII，最多 31 个字符，0 结尾）。
const FN_NAME_LEN: usize = 32;

/// 一个加载了模块的 PawnIO 句柄。每个句柄只能装一个模块。
pub struct PawnIo(HANDLE);

// SAFETY: 句柄只在持有它的线程里用（读数线程），驱动本身对并发调用是安全的。
unsafe impl Send for PawnIo {}

impl PawnIo {
    /// 打开驱动并加载模块。驱动没装时报“没有安装 PawnIO”；模块不支持这颗 CPU 时加载失败。
    pub fn load(module: &[u8]) -> Result<Self> {
        let path: Vec<u16> = DEVICE.encode_utf16().chain([0]).collect();
        // SAFETY: 路径以 0 结尾；失败时返回 INVALID_HANDLE_VALUE。
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            // SAFETY: 紧跟在失败的调用之后。
            let code = unsafe { GetLastError() };
            match code {
                2 | 3 => bail!("没有安装 PawnIO 驱动"),
                5 => bail!("打不开 PawnIO 驱动（需要管理员或 SYSTEM 权限）"),
                _ => bail!("打不开 PawnIO 驱动（错误 {code}）"),
            }
        }
        let io = Self(handle);
        let mut returned = 0u32;
        // SAFETY: 输入缓冲区就是模块字节，没有输出。
        let ok = unsafe {
            DeviceIoControl(
                io.0,
                IOCTL_LOAD_BINARY,
                module.as_ptr().cast(),
                module.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            // SAFETY: 同上。
            let code = unsafe { GetLastError() };
            bail!("PawnIO 模块加载失败（错误 {code}，可能不支持这颗 CPU）");
        }
        Ok(io)
    }

    /// 调用模块里的函数：输入、输出都是 64 位整数。
    pub fn execute(&self, name: &str, input: &[u64], outputs: usize) -> Result<Vec<u64>> {
        let mut buf = vec![0u8; FN_NAME_LEN + input.len() * 8];
        let n = name.len().min(FN_NAME_LEN - 1);
        buf[..n].copy_from_slice(&name.as_bytes()[..n]);
        for (i, v) in input.iter().enumerate() {
            buf[FN_NAME_LEN + i * 8..FN_NAME_LEN + i * 8 + 8].copy_from_slice(&v.to_le_bytes());
        }
        let mut out = vec![0u64; outputs];
        let mut returned = 0u32;
        // SAFETY: 两个缓冲区的长度都按实际大小传。
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                IOCTL_EXECUTE_FN,
                buf.as_ptr().cast(),
                buf.len() as u32,
                out.as_mut_ptr().cast(),
                (outputs * 8) as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            // SAFETY: 同上。
            let code = unsafe { GetLastError() };
            bail!("{name} 调用失败（错误 {code}）");
        }
        out.truncate(returned as usize / 8);
        Ok(out)
    }

    pub fn read_msr(&self, msr: u32) -> Result<u64> {
        let out = self.execute("ioctl_read_msr", &[u64::from(msr)], 1)?;
        out.first().copied().ok_or_else(|| anyhow::anyhow!("读 MSR {msr:#x} 没有返回值"))
    }
}

impl Drop for PawnIo {
    fn drop(&mut self) {
        // SAFETY: 句柄由 CreateFileW 打开，只关一次。
        unsafe { CloseHandle(self.0) };
    }
}
