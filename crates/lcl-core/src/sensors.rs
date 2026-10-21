//! CPU 温度和功耗。读处理器寄存器要内核驱动（PawnIO），而驱动只给管理员用，
//! 所以由以 SYSTEM 身份运行的 lcl-sensord 服务读，经这个命名管道交给后台服务：
//! 每个连接写一行 JSON 就关掉。服务没装或没运行时读不到，返回 None。

use serde::{Deserialize, Serialize};
use std::io::Read;

pub const PIPE_PATH: &str = r"\\.\pipe\lconnect-lite-sensors";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CpuSensors {
    /// 封装温度（Intel 的 Package / AMD 的 Tctl）。
    pub temp_c: Option<f32>,
    /// 封装功耗，按 RAPL 能量计数器两次读数之差算。
    pub power_w: Option<f32>,
    /// 用的哪个 PawnIO 模块，比如“IntelMSR”。
    pub source: String,
    /// 驱动没装、模块不支持这颗 CPU 之类的原因。
    pub error: Option<String>,
}

/// 向 lcl-sensord 要一次最新读数。服务端写完一行就关管道；关闭的方式不同，
/// 最后一次读可能报错而不是返回 EOF，所以只要读到完整的一行就算成功。
pub fn read() -> Option<CpuSensors> {
    let mut pipe = std::fs::File::open(PIPE_PATH).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
        if buf.contains(&b'\n') {
            break;
        }
    }
    let text = std::str::from_utf8(&buf).ok()?;
    serde_json::from_str(text.lines().next()?.trim()).ok()
}
