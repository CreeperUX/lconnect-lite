//! GUI 后端：把前端的 invoke 转成发给 lcl-daemon 的命名管道请求，另外负责文件对话框和拉起后台服务。

use lcl_core::gpu::Gpu;
use lcl_core::{ipc, Config, Request, Response, Status};
use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// 命名管道调用是阻塞的，放到后台线程，避免卡住界面。
async fn call(req: Request) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || ipc::request(&req))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

fn unexpected(resp: Response) -> String {
    match resp {
        Response::Error { message } => message,
        other => format!("后台服务回复了意外的内容：{other:?}"),
    }
}

async fn expect_ok(req: Request) -> Result<(), String> {
    match call(req).await? {
        Response::Ok => Ok(()),
        other => Err(unexpected(other)),
    }
}

#[tauri::command]
async fn get_status() -> Result<Status, String> {
    match call(Request::GetStatus).await? {
        Response::Status { mut status } => {
            fill_gpu(&mut status);
            Ok(status)
        }
        other => Err(unexpected(other)),
    }
}

/// 后台服务只在控制需要时才读 GPU（nvml.dll 加载后卸不掉，约 20 MB）。
/// 它没给读数时由界面进程自己读来显示；界面关掉后整个进程连同 NVML 一起释放。
fn fill_gpu(status: &mut Status) {
    static GPU: OnceLock<Mutex<Option<Gpu>>> = OnceLock::new();
    if status.gpu_temp_c.is_some() {
        return;
    }
    let gpu = GPU.get_or_init(|| Mutex::new(Gpu::open()));
    if let Some(sample) = gpu.lock().unwrap().as_ref().map(Gpu::sample) {
        status.gpu_temp_c = sample.temp_c;
        status.gpu_usage = sample.usage;
        status.gpu_power_w = sample.power_w;
    }
}

#[tauri::command]
async fn get_config() -> Result<Config, String> {
    match call(Request::GetConfig).await? {
        Response::Config { config } => Ok(config),
        other => Err(unexpected(other)),
    }
}

#[tauri::command]
async fn set_config(config: Config) -> Result<(), String> {
    expect_ok(Request::SetConfig { config }).await
}

#[tauri::command]
async fn get_lighting_caps() -> Result<lcl_core::ipc::LightingCaps, String> {
    match call(Request::GetLightingCaps).await? {
        Response::LightingCaps { caps } => Ok(caps),
        other => Err(unexpected(other)),
    }
}

#[tauri::command]
async fn get_lcd_state() -> Result<lcl_core::ipc::LcdState, String> {
    match call(Request::GetLcdState).await? {
        Response::LcdState { state } => Ok(state),
        other => Err(unexpected(other)),
    }
}

#[tauri::command]
async fn bind_device(address: String) -> Result<(), String> {
    expect_ok(Request::Bind { address }).await
}

#[tauri::command]
async fn unbind_device(address: String) -> Result<(), String> {
    expect_ok(Request::Unbind { address }).await
}

#[tauri::command]
async fn set_mb_light_sync(address: String, enabled: bool) -> Result<(), String> {
    expect_ok(Request::SetMbLightSync { address, enabled }).await
}

/// 选屏幕要显示的图片或 GIF，返回路径；取消返回 None。
#[tauri::command]
async fn pick_media(app: AppHandle, kind: String) -> Result<Option<String>, String> {
    let (name, exts): (&str, &[&str]) = if kind == "gif" {
        ("GIF 动图", &["gif"])
    } else {
        ("图片", &["png", "jpg", "jpeg", "bmp", "webp"])
    };
    let picked = app.dialog().file().add_filter(name, exts).blocking_pick_file();
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
async fn get_autostart() -> Result<bool, String> {
    match call(Request::GetAutostart).await? {
        Response::Autostart { enabled } => Ok(enabled),
        other => Err(unexpected(other)),
    }
}

#[tauri::command]
async fn set_autostart(enabled: bool) -> Result<(), String> {
    expect_ok(Request::SetAutostart { enabled }).await
}

/// 弹出保存对话框导出当前配置；用户取消返回 None。
#[tauri::command]
async fn export_config(app: AppHandle) -> Result<Option<String>, String> {
    let text = get_config().await?.to_toml().map_err(|e| format!("{e:#}"))?;
    let picked = app
        .dialog()
        .file()
        .add_filter("LConnect Lite 配置", &["toml"])
        .set_file_name("lconnect-lite.toml")
        .blocking_save_file();
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("写入 {} 失败：{e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

/// 弹出打开对话框，读入配置并交给后台服务应用；用户取消返回 None。
#[tauri::command]
async fn import_config(app: AppHandle) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("LConnect Lite 配置", &["toml"])
        .blocking_pick_file();
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    let config = Config::from_toml(&text).map_err(|e| format!("{e:#}"))?;
    expect_ok(Request::SetConfig { config }).await?;
    Ok(Some(path.display().to_string()))
}

/// 启动和界面放在同一目录的 lcl-daemon.exe。
#[tauri::command]
fn start_daemon() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let daemon = exe.with_file_name("lcl-daemon.exe");
    std::process::Command::new(&daemon)
        .arg("serve")
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("启动 {} 失败：{e}", daemon.display()))
}

#[derive(Serialize)]
struct Paths {
    config: String,
    log: String,
}

#[tauri::command]
fn paths() -> Paths {
    Paths {
        config: Config::path().display().to_string(),
        log: lcl_core::config::log_path().display().to_string(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_config,
            set_config,
            get_lighting_caps,
            get_lcd_state,
            pick_media,
            bind_device,
            unbind_device,
            set_mb_light_sync,
            get_autostart,
            set_autostart,
            export_config,
            import_config,
            start_daemon,
            paths
        ])
        .run(tauri::generate_context!())
        .expect("启动 LConnect Lite 界面失败");
}
