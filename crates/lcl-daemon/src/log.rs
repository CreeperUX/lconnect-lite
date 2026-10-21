//! 日志：`%LOCALAPPDATA%\LConnect Lite\daemon.log`，超过 1 MB 时改名为 daemon.log.old 再重新开始。
//! release 版没有控制台，日志是唯一的输出；debug 版同时打到 stderr。

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

const MAX_LEN: u64 = 1 << 20;
static LOCK: Mutex<()> = Mutex::new(());

pub fn line(msg: &str) {
    let _guard = LOCK.lock();
    // SAFETY: GetLocalTime 只写入传入的结构体。
    let t = unsafe {
        let mut t: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    let text = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {msg}\n",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    );
    eprint!("{text}");
    let path = lcl_core::config::log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LEN) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(text.as_bytes());
    }
}
