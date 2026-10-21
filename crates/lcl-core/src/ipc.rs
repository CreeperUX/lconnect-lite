//! 后台服务与 GUI 之间的命名管道协议：每行一个 JSON 请求，对应一行 JSON 回复。
//! 管道名 `\\.\pipe\lconnect-lite`。

use crate::{Config, Status};
use lcl_effects::types::RgbEffectParameters;
use anyhow::{Context, Result};
use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Stream};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};

pub const PIPE_NAME: &str = "lconnect-lite";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    GetStatus,
    GetConfig,
    SetConfig { config: Config },
    GetAutostart,
    SetAutostart { enabled: bool },
    /// 各类设备支持的灯效和每种效果可调的参数。
    GetLightingCaps,
    /// 屏幕当前在显示什么，以及最近一帧画面（JPEG，base64）。
    GetLcdState,
    /// 把无线设备绑定到本机发射器 / 解绑。由控制线程执行，最多等 15 秒。
    Bind { address: String },
    Unbind { address: String },
    /// 开关某个本机设备的主板灯光同步。
    SetMbLightSync { address: String, enabled: bool },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LcdState {
    /// 例如“仪表盘”“图片：xxx.png”“GIF：xxx.gif（42 帧）”“未控制”。
    pub description: String,
    pub error: Option<String>,
    /// 最近一次推到屏幕上的 JPEG，base64 编码。
    pub preview_jpeg_base64: Option<String>,
}

/// 灯效能力：每种效果的颜色数、方向、能否调速。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingCaps {
    /// 本机绑定的无线风扇组里第一个组的风扇数（Static/Breathing 每个风扇一种颜色）。
    pub fan_count: u8,
    pub fans: Vec<RgbEffectParameters>,
    pub pump: Vec<RgbEffectParameters>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Status { status: Status },
    Config { config: Config },
    Autostart { enabled: bool },
    LightingCaps { caps: LightingCaps },
    LcdState { state: LcdState },
    Ok,
    Error { message: String },
}

/// 客户端：连上管道、发一个请求、等一行回复。
pub fn request(req: &Request) -> Result<Response> {
    let name = PIPE_NAME.to_ns_name::<GenericNamespaced>()?;
    let conn = Stream::connect(name).context("连不上后台服务（lcl-daemon 没在运行？）")?;
    let mut conn = BufReader::new(conn);
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    conn.get_mut().write_all(line.as_bytes())?;
    let mut reply = String::new();
    conn.read_line(&mut reply)?;
    Ok(serde_json::from_str(&reply)?)
}

/// 服务端：阻塞监听，每个连接一个线程，逐行交给 `handler`。
pub fn serve<F>(handler: F) -> Result<()>
where
    F: Fn(Request) -> Response + Send + Sync + 'static,
{
    let name = PIPE_NAME.to_ns_name::<GenericNamespaced>()?;
    let listener = ListenerOptions::new()
        .name(name)
        .create_sync()
        .context("创建命名管道失败（已经有一个 lcl-daemon 在运行？）")?;
    let handler = std::sync::Arc::new(handler);
    for conn in listener.incoming() {
        let Ok(conn) = conn else { continue };
        let handler = std::sync::Arc::clone(&handler);
        std::thread::spawn(move || {
            let mut conn = BufReader::new(conn);
            let mut line = String::new();
            while matches!(conn.read_line(&mut line), Ok(n) if n > 0) {
                let resp = match serde_json::from_str::<Request>(&line) {
                    Ok(req) => handler(req),
                    Err(e) => Response::Error { message: format!("无法解析请求：{e}") },
                };
                let mut out = serde_json::to_string(&resp).unwrap_or_default();
                out.push('\n');
                if conn.get_mut().write_all(out.as_bytes()).is_err() {
                    break;
                }
                line.clear();
            }
        });
    }
    Ok(())
}
