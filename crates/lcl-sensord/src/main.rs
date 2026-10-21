//! LConnect Lite 传感器服务：以 SYSTEM 身份运行（Windows 服务“LConnectLiteSensors”），
//! 每秒经 PawnIO 读一次 CPU 封装温度和功耗，通过命名管道交给以普通用户身份运行的后台服务。
//!
//! `lcl-sensord --console`：不经服务管理器，直接在控制台打印读数（要以管理员身份运行），调试用。

#![windows_subsystem = "console"]

mod cpu;
mod pawnio;
mod pipe;

use lcl_core::sensors::CpuSensors;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::NO_ERROR;
use windows_sys::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW, SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP,
    SERVICE_CONTROL_INTERROGATE, SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP, SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STATUS,
    SERVICE_STATUS_CURRENT_STATE, SERVICE_STOPPED, SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
};

const SERVICE_NAME: &str = "LConnectLiteSensors";
const INTERVAL: Duration = Duration::from_secs(1);
/// 驱动没装或打不开时，隔多久再试（装上 PawnIO 后不用重启服务）。
const RETRY: Duration = Duration::from_secs(30);

static STOP: AtomicBool = AtomicBool::new(false);
static STATUS_HANDLE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

fn main() {
    if std::env::args().any(|a| a == "--console") {
        console();
        return;
    }
    let mut name: Vec<u16> = SERVICE_NAME.encode_utf16().chain([0]).collect();
    let table = [
        SERVICE_TABLE_ENTRYW { lpServiceName: name.as_mut_ptr(), lpServiceProc: Some(service_main) },
        SERVICE_TABLE_ENTRYW { lpServiceName: std::ptr::null_mut(), lpServiceProc: None },
    ];
    // SAFETY: 表以空项结尾；不是由服务管理器启动时立即返回失败。
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        eprintln!("这是 Windows 服务，由安装程序注册；调试请用 --console（管理员）。");
    }
}

/// 读数线程：一直更新 `latest`，直到 STOP。
fn sample_loop(latest: Arc<Mutex<CpuSensors>>) {
    while !STOP.load(Ordering::SeqCst) {
        match cpu::Cpu::open() {
            Ok(mut cpu) => {
                while !STOP.load(Ordering::SeqCst) {
                    *latest.lock().unwrap() = cpu.sample();
                    std::thread::sleep(INTERVAL);
                }
            }
            Err(e) => {
                *latest.lock().unwrap() = CpuSensors { error: Some(format!("{e:#}")), ..Default::default() };
                let mut waited = Duration::ZERO;
                while waited < RETRY && !STOP.load(Ordering::SeqCst) {
                    std::thread::sleep(INTERVAL);
                    waited += INTERVAL;
                }
            }
        }
    }
}

fn console() {
    let latest = Arc::new(Mutex::new(CpuSensors::default()));
    let l = Arc::clone(&latest);
    std::thread::spawn(move || sample_loop(l));
    loop {
        std::thread::sleep(INTERVAL);
        println!("{:?}", *latest.lock().unwrap());
    }
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut PWSTR) {
    let name: Vec<u16> = SERVICE_NAME.encode_utf16().chain([0]).collect();
    // SAFETY: 名字以 0 结尾；处理函数是 'static 的 extern 函数。
    let handle = unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control_handler), std::ptr::null()) };
    if handle.is_null() {
        return;
    }
    STATUS_HANDLE.store(handle, Ordering::SeqCst);
    report(SERVICE_START_PENDING);

    let latest = Arc::new(Mutex::new(CpuSensors::default()));
    {
        let latest = Arc::clone(&latest);
        std::thread::spawn(move || sample_loop(latest));
    }
    {
        let latest = Arc::clone(&latest);
        std::thread::spawn(move || {
            // 管道建不起来（比如名字被占）时服务照样运行，只是没人能读。
            let _ = pipe::serve(&latest);
        });
    }
    report(SERVICE_RUNNING);
    while !STOP.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(200));
    }
    report(SERVICE_STOPPED);
}

unsafe extern "system" fn control_handler(control: u32, _event: u32, _data: *mut core::ffi::c_void, _ctx: *mut core::ffi::c_void) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => STOP.store(true, Ordering::SeqCst),
        SERVICE_CONTROL_INTERROGATE => {}
        _ => {}
    }
    NO_ERROR
}

fn report(state: SERVICE_STATUS_CURRENT_STATE) {
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING { SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN } else { 0 },
        dwWin32ExitCode: NO_ERROR,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: if state == SERVICE_START_PENDING { 3000 } else { 0 },
    };
    // SAFETY: 句柄由 RegisterServiceCtrlHandlerExW 返回。
    unsafe { SetServiceStatus(STATUS_HANDLE.load(Ordering::SeqCst), &status) };
}
