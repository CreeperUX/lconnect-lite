//! 开机自启：HKCU\...\Run 下的 "LConnect Lite"，登录时启动 `lcl-daemon.exe serve`。不需要管理员权限。

use anyhow::{Context, Result};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "LConnect Lite";

fn command() -> Result<String> {
    let exe = std::env::current_exe().context("取不到 lcl-daemon.exe 的路径")?;
    Ok(format!("\"{}\" serve", exe.display()))
}

/// 已登记且指向当前这个 exe 才算开启（程序搬过位置后视为未开启）。
pub fn is_enabled() -> bool {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ) else {
        return false;
    };
    match (key.get_value::<String, _>(VALUE), command()) {
        (Ok(v), Ok(cmd)) => v.eq_ignore_ascii_case(&cmd),
        _ => false,
    }
}

pub fn set(enabled: bool) -> Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE)
        .context("打开注册表 Run 键失败")?;
    if enabled {
        key.set_value(VALUE, &command()?).context("写入开机自启项失败")?;
    } else {
        match key.delete_value(VALUE) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).context("删除开机自启项失败"),
        }
    }
    Ok(())
}
