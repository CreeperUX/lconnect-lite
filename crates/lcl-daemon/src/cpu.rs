//! 整机 CPU 占用率：两次 GetSystemTimes 之差，不需要内核驱动。
//! 用作风扇控制的前馈信号（功耗的近似），也显示在概览页。

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::Threading::GetSystemTimes;

#[derive(Default)]
pub struct CpuUsage {
    /// 上一次采样的 (空闲, 总计)，单位 100 ns。
    last: Option<(u64, u64)>,
}

impl CpuUsage {
    /// 返回自上次调用以来的占用率（%）；第一次调用只记录基准，返回 None。
    pub fn sample(&mut self) -> Option<f32> {
        let (idle, total) = system_times()?;
        let prev = self.last.replace((idle, total));
        let (prev_idle, prev_total) = prev?;
        let d_total = total.checked_sub(prev_total)?;
        let d_idle = idle.checked_sub(prev_idle)?;
        if d_total == 0 {
            return None;
        }
        Some((1.0 - d_idle as f32 / d_total as f32).clamp(0.0, 1.0) * 100.0)
    }
}

fn system_times() -> Option<(u64, u64)> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut idle, mut kernel, mut user) = (zero, zero, zero);
    // SAFETY: 三个指针都指向本函数栈上有效的 FILETIME。
    let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    if ok == 0 {
        return None;
    }
    let ft = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    // 内核时间里已经包含了空闲时间。
    Some((ft(idle), ft(kernel) + ft(user)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_sample_is_a_percentage() {
        let mut cpu = CpuUsage::default();
        assert_eq!(cpu.sample(), None);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let v = cpu.sample().expect("GetSystemTimes 应该可用");
        assert!((0.0..=100.0).contains(&v));
    }
}
