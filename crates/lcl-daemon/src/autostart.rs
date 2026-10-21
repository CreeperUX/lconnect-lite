//! 开机自启：计划任务“LConnect Lite”，当前用户登录时立即启动 `lcl-daemon.exe serve`。不需要管理员权限。
//!
//! 以前用 HKCU\...\Run：那里的程序要等桌面起来后再被系统延迟约 10 秒才启动，这段时间风扇按开机接管的设置跑
//! （或者满转）。计划任务的登录触发没有这段延迟。开关时顺手清掉旧版本留下的 Run 项。

use anyhow::{bail, Context, Result};
use std::os::windows::process::CommandExt;
use std::process::Command;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const TASK: &str = "LConnect Lite";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// 不弹控制台窗口（后台服务没有控制台，直接起 schtasks 会闪一下黑框）。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn exe() -> Result<String> {
    Ok(std::env::current_exe().context("取不到 lcl-daemon.exe 的路径")?.display().to_string())
}

fn schtasks(args: &[&str]) -> Result<std::process::Output> {
    Command::new("schtasks").args(args).creation_flags(CREATE_NO_WINDOW).output().context("运行 schtasks 失败")
}

/// 任务存在且启动的是当前这个 exe 才算开启（程序搬过位置后视为未开启）。
pub fn is_enabled() -> bool {
    let (Ok(out), Ok(exe)) = (schtasks(&["/query", "/tn", TASK, "/xml"]), exe()) else {
        return false;
    };
    out.status.success() && String::from_utf8_lossy(&out.stdout).to_lowercase().contains(&xml_escape(&exe).to_lowercase())
}

pub fn set(enabled: bool) -> Result<()> {
    remove_run_value();
    if !enabled {
        let out = schtasks(&["/delete", "/tn", TASK, "/f"])?;
        // 本来就没有任务时也算成功。
        if !out.status.success() && schtasks(&["/query", "/tn", TASK])?.status.success() {
            bail!("删除开机自启任务失败：{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        return Ok(());
    }
    // schtasks /xml 要 UTF-16 的文件。
    let path = std::env::temp_dir().join("lconnect-lite-autostart.xml");
    let text = task_xml(&exe()?, &current_user());
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&path, bytes).context("写计划任务定义失败")?;
    let out = schtasks(&["/create", "/tn", TASK, "/xml", &path.display().to_string(), "/f"]);
    let _ = std::fs::remove_file(&path);
    let out = out?;
    if !out.status.success() {
        bail!("创建开机自启任务失败：{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

/// 旧版本写在 Run 里的同名项。
fn remove_run_value() {
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE) {
        let _ = key.delete_value(TASK);
    }
}

fn current_user() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => format!("{domain}\\{user}"),
        _ => user,
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// 只在这个用户登录时、以普通权限运行；不受电池限制，不设运行时长上限（默认 72 小时会被杀掉），已在运行就不再起。
fn task_xml(exe: &str, user: &str) -> String {
    let (exe, user) = (xml_escape(exe), xml_escape(user));
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>LConnect Lite 后台服务：登录后立即启动，控制风扇、泵、灯效和冷头屏幕。</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>"{exe}"</Command>
      <Arguments>serve</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_xml_escapes_and_has_no_time_limit() {
        let xml = task_xml(r"C:\A&B\lcl-daemon.exe", r"PC\me");
        assert!(xml.contains(r#"<Command>"C:\A&amp;B\lcl-daemon.exe"</Command>"#));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains(r"<UserId>PC\me</UserId>"));
    }
}
