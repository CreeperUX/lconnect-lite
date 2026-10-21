//! 托盘图标：提示文字显示实时读数，右键菜单切预设、开关自启、打开界面、退出。
//! tray-icon 要求创建它的线程跑 Win32 消息循环，所以在主线程运行，控制循环在另一个线程。

use crate::{autostart, log};
use lcl_core::presets::PRESETS;
use lcl_core::{Config, Status};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};

/// 托盘需要的两件事：读当前状态/配置、写配置。
pub trait Host {
    fn snapshot(&self) -> (Status, Config);
    fn set_config(&self, config: Config);
}

/// 运行到 `stop` 置位（菜单“退出”也会置位）。图标创建失败（比如没有桌面会话）时只是等待退出。
pub fn run(host: &dyn Host, stop: &AtomicBool) {
    native_look();
    // 第一行是实时读数，不可点。
    let reading = MenuItem::new("LConnect Lite", false, None);
    let open = MenuItem::new("打开 LConnect Lite", true, None);
    let presets: Vec<CheckMenuItem> = PRESETS.iter().map(|p| CheckMenuItem::new(p.name, true, false, None)).collect();
    let auto = CheckMenuItem::new("开机自启", true, autostart::is_enabled(), None);
    let quit = MenuItem::new("退出后台服务", true, None);

    let menu = Menu::new();
    let sep0 = PredefinedMenuItem::separator();
    let mut items: Vec<&dyn tray_icon::menu::IsMenuItem> = vec![&reading, &sep0, &open];
    let sep1 = PredefinedMenuItem::separator();
    let sep2 = PredefinedMenuItem::separator();
    items.push(&sep1);
    for p in &presets {
        items.push(p);
    }
    items.push(&sep2);
    items.push(&auto);
    items.push(&quit);
    if let Err(e) = menu.append_items(&items) {
        log::line(&format!("托盘菜单创建失败：{e}"));
    }

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("LConnect Lite")
        .with_icon(icon())
        .build();
    let tray = match tray {
        Ok(t) => Some(t),
        Err(e) => {
            log::line(&format!("托盘图标创建失败，继续无托盘运行：{e}"));
            None
        }
    };

    let mut last_refresh = Instant::now() - Duration::from_secs(10);
    while !stop.load(Ordering::SeqCst) {
        pump_messages();

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == open.id() {
                open_gui();
            } else if ev.id == quit.id() {
                stop.store(true, Ordering::SeqCst);
            } else if ev.id == auto.id() {
                let want = auto.is_checked();
                if let Err(e) = autostart::set(want) {
                    log::line(&format!("{e:#}"));
                }
                auto.set_checked(autostart::is_enabled());
            } else if let Some(i) = presets.iter().position(|p| ev.id == *p.id()) {
                let (_, mut config) = host.snapshot();
                PRESETS[i].apply(&mut config);
                host.set_config(config);
                last_refresh = Instant::now() - Duration::from_secs(10);
            }
        }

        if last_refresh.elapsed() >= Duration::from_secs(2) {
            last_refresh = Instant::now();
            let (status, config) = host.snapshot();
            for (item, preset) in presets.iter().zip(PRESETS.iter()) {
                item.set_checked(preset.matches(&config));
            }
            if let Some(tray) = &tray {
                let _ = tray.set_tooltip(Some(tooltip(&status)));
            }
            reading.set_text(reading_line(&status));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn tooltip(s: &Status) -> String {
    let f = |v: Option<f32>, d: usize| v.map_or("—".to_string(), |v| format!("{v:.d$}"));
    let mut text = format!(
        "LConnect Lite\n水温 {} °C · 泵 {} rpm\n风扇 {}%",
        f(s.coolant_c, 1),
        s.pump_rpm.map_or("—".to_string(), |v| v.to_string()),
        f(s.fan_percent, 0)
    );
    if s.last_error.is_some() {
        text.push_str("\n有错误，打开界面查看");
    }
    // Windows 托盘提示最多 127 个字符。
    text.chars().take(120).collect()
}

fn reading_line(s: &Status) -> String {
    let coolant = s.coolant_c.map_or("—".to_string(), |v| format!("{v:.1}"));
    let pump = s.pump_rpm.map_or("—".to_string(), |v| v.to_string());
    let fan = s.fan_percent.map_or("—".to_string(), |v| format!("{v:.0}"));
    format!("水温 {coolant} °C · 泵 {pump} · 风扇 {fan}%")
}

/// 托盘菜单是系统原生菜单。后台服务默认不声明 DPI 感知，高缩放下系统会把菜单按位图放大，字是糊的；
/// 这里声明按显示器感知 DPI，再让原生菜单用深色（和界面一致）。
fn native_look() {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    // SAFETY: 两个调用都只改本进程的设置。uxtheme 的 SetPreferredAppMode（序号 135）和 FlushMenuThemes（序号 136）
    // 没有公开声明，但从 Windows 10 1903 起稳定存在，资源管理器和 muda 的深色菜单栏也用它们；找不到就跳过。
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let name: Vec<u16> = "uxtheme.dll".encode_utf16().chain([0]).collect();
        let lib = LoadLibraryW(name.as_ptr());
        if lib.is_null() {
            return;
        }
        if let Some(f) = GetProcAddress(lib, 135 as _) {
            // 2 = ForceDark
            let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            set_mode(2);
        }
        if let Some(f) = GetProcAddress(lib, 136 as _) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(f);
            flush();
        }
    }
}

fn pump_messages() {
    // SAFETY: 标准的 Win32 消息泵，MSG 在栈上，只处理本线程的消息。
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// 打开和 lcl-daemon.exe 放在同一目录下的 lcl-app.exe。
fn open_gui() {
    let Ok(exe) = std::env::current_exe() else { return };
    let app = exe.with_file_name("lcl-app.exe");
    if let Err(e) = std::process::Command::new(&app).spawn() {
        log::line(&format!("打开界面失败（{}）：{e}", app.display()));
    }
}

/// 32×32 图标：和应用图标同一个造型（app/public/logo.svg 栅格化后的原始 RGBA）。
fn icon() -> Icon {
    const RGBA: &[u8] = include_bytes!("../assets/tray-32.rgba");
    Icon::from_rgba(RGBA.to_vec(), 32, 32).expect("图标尺寸正确")
}
