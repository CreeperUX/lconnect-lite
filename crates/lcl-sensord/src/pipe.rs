//! 命名管道服务端：每个连接写一行 JSON（最新读数）就关掉。
//! 管道权限：SYSTEM 和管理员完全控制，登录的用户只能读，后台服务以普通用户身份也能连。

use anyhow::{bail, Result};
use lcl_core::sensors::{CpuSensors, PIPE_PATH};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows_sys::Win32::Storage::FileSystem::{FlushFileBuffers, WriteFile, PIPE_ACCESS_OUTBOUND};
use windows_sys::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT};

/// SYSTEM、管理员：完全控制；已登录用户：读。
const SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GR;;;AU)";

/// 一直服务下去（进程退出时结束）。
pub fn serve(latest: &Mutex<CpuSensors>) -> Result<()> {
    let sddl: Vec<u16> = SDDL.encode_utf16().chain([0]).collect();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: 字符串以 0 结尾；成功时 sd 由系统分配，进程结束前一直用，不释放。
    if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) } == 0 {
        bail!("创建管道权限失败（错误 {}）", unsafe { GetLastError() });
    }
    let attrs = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd, bInheritHandle: 0 };
    let name: Vec<u16> = PIPE_PATH.encode_utf16().chain([0]).collect();
    loop {
        // SAFETY: 参数都是有效值；失败时返回 INVALID_HANDLE_VALUE。
        let pipe = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_OUTBOUND,
                PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                4096,
                0,
                0,
                &attrs,
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            // SAFETY: 紧跟在失败的调用之后。
            let code = unsafe { GetLastError() };
            // SAFETY: sd 由 ConvertStringSecurityDescriptor 分配。
            unsafe { LocalFree(sd) };
            bail!("创建命名管道失败（错误 {code}）");
        }
        // SAFETY: 阻塞等一个客户端；客户端在两次调用之间连上时返回 ERROR_PIPE_CONNECTED，也算连上。
        let connected = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0 || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if connected {
            let mut line = serde_json::to_string(&*latest.lock().unwrap()).unwrap_or_default();
            line.push('\n');
            let mut written = 0u32;
            // SAFETY: 缓冲区长度按实际传；刷新等客户端读完。之后直接关句柄（不调 DisconnectNamedPipe），
            // 客户端读到的是正常的 EOF，而不是“管道未连接”错误。
            unsafe {
                WriteFile(pipe, line.as_ptr(), line.len() as u32, &mut written, std::ptr::null_mut());
                FlushFileBuffers(pipe);
            }
        }
        // SAFETY: 每个实例只关一次。
        unsafe { CloseHandle(pipe) };
    }
}
