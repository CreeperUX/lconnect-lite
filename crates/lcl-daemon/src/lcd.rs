//! 屏幕线程：按配置准备画面（传感器主题 / 图片 / GIF），推到冷头屏幕上。
//! 冷头句柄和控制循环共用（`H2Slot`）；冷头重新打开或推送灯圈后屏幕要重新初始化，
//! 靠 `H2Slot::generation` 发现。选“不控制”时完全不碰屏幕。
//! 屏幕有两层：JPEG 底层（我们推的画面）和 PNG 叠加层，初始化时把叠加层清成透明。

use crate::h2slot::H2Slot;
use crate::log;
use lcl_core::config::{Lcd, LcdMode};
use lcl_core::Status;
use lcl_devices::h2::ClockMode;
use lcl_media::theme::{self, rgb, Style, ThemeData};
use lcl_media::Frame;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const DASHBOARD_INTERVAL: Duration = Duration::from_secs(1);
/// 水温走势：每 10 秒记一个点，保留 10 分钟。
const HISTORY_EVERY: Duration = Duration::from_secs(10);
const HISTORY_LEN: usize = 60;
/// “稳定 / 上升 / 下降”看两分钟内的变化。
const TREND_SAMPLES: usize = 12;
/// 静态图片隔这么久重推一次，防止设备自己切回别的画面。
const STILL_REFRESH: Duration = Duration::from_secs(30);
const IDLE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Default)]
pub struct LcdState {
    pub description: String,
    pub error: Option<String>,
    /// 最近一次推到屏幕上的 JPEG。
    pub preview: Option<Vec<u8>>,
}

enum Content {
    None,
    Still(Vec<u8>),
    Anim(Vec<Frame>),
    Theme,
}

/// 最近十分钟的水温，给走势线和趋势标签用。
#[derive(Default)]
struct History {
    samples: VecDeque<f32>,
    last: Option<Instant>,
}

impl History {
    fn push(&mut self, coolant: Option<f32>) {
        let Some(c) = coolant else { return };
        if self.last.is_some_and(|t| t.elapsed() < HISTORY_EVERY) {
            return;
        }
        self.last = Some(Instant::now());
        self.samples.push_back(c);
        while self.samples.len() > HISTORY_LEN {
            self.samples.pop_front();
        }
    }

    fn trend(&self) -> Option<f32> {
        let n = self.samples.len();
        (n > 1).then(|| self.samples[n - 1] - self.samples[n.saturating_sub(TREND_SAMPLES + 1)])
    }
}

pub fn run(h2: &H2Slot, stop: &AtomicBool, snapshot: impl Fn() -> (Lcd, Status), publish: impl Fn(LcdState)) {
    let mut history = History::default();
    let mut content = Content::None;
    let mut prepared: Option<String> = None;
    let mut state = LcdState::default();
    let mut inited_gen: Option<u64> = None;
    let mut applied: Option<(u8, bool)> = None;
    let mut next_due = Instant::now();
    let mut frame_idx = 0usize;
    let mut last_dash: Option<String> = None;

    while !stop.load(Ordering::SeqCst) {
        let (cfg, status) = snapshot();
        history.push(status.coolant_c);

        if cfg.mode == LcdMode::Unmanaged {
            if prepared.is_some() || state.description.is_empty() {
                prepared = None;
                content = Content::None;
                inited_gen = None;
                state = LcdState { description: "未控制（保持设备现有画面）".into(), ..Default::default() };
                publish(state.clone());
            }
            std::thread::sleep(IDLE);
            continue;
        }

        // 画面来源变了：重新准备（读文件、解码、压 JPEG），这一步不占冷头。
        let key = format!("{:?}|{}|{}|{}|{}", cfg.mode, cfg.media_path, cfg.rotation, cfg.max_fps, cfg.theme);
        if prepared.as_deref() != Some(key.as_str()) {
            prepared = Some(key);
            frame_idx = 0;
            next_due = Instant::now();
            last_dash = None;
            state.error = None;
            content = match prepare(&cfg) {
                Ok((c, desc)) => {
                    state.description = desc;
                    c
                }
                Err(e) => {
                    state.description = "画面准备失败".into();
                    state.error = Some(format!("{e:#}"));
                    log::line(&format!("屏幕：{e:#}"));
                    Content::None
                }
            };
            publish(state.clone());
        }

        let wait = {
            let gen = h2.generation();
            let mut slot = h2.lock();
            match slot.as_mut() {
                None => IDLE,
                Some(dev) => {
                    let result = (|| -> anyhow::Result<Duration> {
                        if inited_gen != Some(gen) {
                            dev.lcd_init()?;
                            // 清掉 PNG 叠加层：L-Connect 主题的文字和仪表留在这层，会盖住我们的画面。
                            dev.push_png(&lcl_media::transparent_png()?)?;
                            inited_gen = Some(gen);
                            applied = None;
                            last_dash = None;
                            next_due = Instant::now();
                        }
                        if applied != Some((cfg.brightness, cfg.offline_clock)) {
                            dev.set_brightness(cfg.brightness)?;
                            dev.set_clock(if cfg.offline_clock { ClockMode::On } else { ClockMode::Off })?;
                            applied = Some((cfg.brightness, cfg.offline_clock));
                        }
                        let now = Instant::now();
                        if now < next_due {
                            return Ok(next_due - now);
                        }
                        match &content {
                            Content::None => Ok(IDLE),
                            Content::Still(jpeg) => {
                                dev.push_jpeg(jpeg)?;
                                set_preview(&mut state, jpeg, &publish);
                                next_due = now + STILL_REFRESH;
                                Ok(IDLE)
                            }
                            Content::Anim(frames) => {
                                let f = &frames[frame_idx % frames.len()];
                                dev.push_jpeg(&f.jpeg)?;
                                if frame_idx == 0 {
                                    set_preview(&mut state, &f.jpeg, &publish);
                                }
                                frame_idx = (frame_idx + 1) % frames.len();
                                next_due = now + Duration::from_millis(u64::from(f.delay_ms));
                                Ok(Duration::from_millis(u64::from(f.delay_ms)))
                            }
                            Content::Theme => {
                                next_due = now + DASHBOARD_INTERVAL;
                                let data = theme_data(&status, &history);
                                let info = theme::find(&cfg.theme);
                                let style = Style {
                                    dark: cfg.theme_mode.is_dark(data.time.0),
                                    accent: cfg.accent.map_or(rgb(info.accent), |[r, g, b]| [r, g, b, 255]),
                                };
                                // 显示的内容和外观都没变就不重画、不重推。
                                let k = format!("{}|{:?}|{}|{}", info.id, style, cfg.rotation, data.key());
                                if last_dash.as_deref() != Some(k.as_str()) {
                                    let jpeg = lcl_media::encode(theme::render(info.id, &data, &style), cfg.rotation)?;
                                    dev.push_jpeg(&jpeg)?;
                                    set_preview(&mut state, &jpeg, &publish);
                                    last_dash = Some(k);
                                }
                                Ok(DASHBOARD_INTERVAL)
                            }
                        }
                    })();
                    match result {
                        Ok(wait) => wait,
                        Err(e) => {
                            // 屏幕命令失败多半是冷头断开或卡住：交给控制循环重新打开。
                            let msg = format!("屏幕：{e:#}");
                            log::line(&msg);
                            state.error = Some(msg);
                            publish(state.clone());
                            *slot = None;
                            inited_gen = None;
                            IDLE
                        }
                    }
                }
            }
        };
        std::thread::sleep(wait.clamp(Duration::from_millis(10), IDLE));
    }
}

fn set_preview(state: &mut LcdState, jpeg: &[u8], publish: &impl Fn(LcdState)) {
    state.preview = Some(jpeg.to_vec());
    state.error = None;
    publish(state.clone());
}

/// 状态快照 → 主题要显示的数据（加上本地时间和水温历史）。
fn theme_data(status: &Status, history: &History) -> ThemeData {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: GetLocalTime 只写入传入的结构体。
    let t = unsafe {
        let mut t: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    ThemeData {
        coolant_c: status.coolant_c,
        coolant_trend: history.trend(),
        coolant_history: history.samples.iter().copied().collect(),
        pump_rpm: status.pump_rpm,
        pump_target_rpm: status.pump_target_rpm,
        fan_percent: status.fan_percent,
        fan_rpm: status.fan_groups.first().map(|g| g.rpm.clone()).unwrap_or_default(),
        cpu_usage: status.cpu_usage,
        cpu_temp_c: status.cpu_temp_c,
        gpu_temp_c: status.gpu_temp_c,
        gpu_usage: status.gpu_usage,
        time: (t.wHour as u8, t.wMinute as u8),
        date: (t.wMonth as u8, t.wDay as u8, t.wDayOfWeek as u8),
    }
}

/// 按配置准备画面，返回内容和一句描述。
fn prepare(cfg: &Lcd) -> anyhow::Result<(Content, String)> {
    let path = Path::new(&cfg.media_path);
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    match cfg.mode {
        LcdMode::Unmanaged => Ok((Content::None, String::new())),
        LcdMode::Dashboard => Ok((Content::Theme, format!("主题：{}", theme::find(&cfg.theme).name))),
        LcdMode::Image => {
            anyhow::ensure!(!cfg.media_path.is_empty(), "还没有选择图片");
            Ok((Content::Still(lcl_media::load_still(path, cfg.rotation)?), format!("图片：{name}")))
        }
        LcdMode::Gif => {
            anyhow::ensure!(!cfg.media_path.is_empty(), "还没有选择 GIF");
            let frames = lcl_media::load_gif(path, cfg.rotation, cfg.max_fps)?;
            let desc = format!("GIF：{name}（{} 帧）", frames.len());
            Ok((Content::Anim(frames), desc))
        }
    }
}
