#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    pub frames: Vec<Vec<[u8; 3]>>,
    pub interval_hundredths: u32,
    pub secondary: Option<SecondaryTiming>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecondaryTiming {
    pub interval_ticks: u16,
    pub frame_count: u16,
    pub outer_longest: bool,
}

impl Animation {
    /// 界面预览用：均匀抽样到最多 `max` 帧，每帧编码成 "rrggbb…" 的十六进制串。
    /// 返回（抽样后的帧间隔毫秒，帧）。设备时钟 1 tick = 0.625 ms，帧间隔以百分之一 tick 计。
    pub fn preview_hex(&self, max: usize) -> (f32, Vec<String>) {
        use std::fmt::Write as _;
        let step = self.frames.len().div_ceil(max.max(1)).max(1);
        let ms = self.interval_hundredths as f32 * 0.00625 * step as f32;
        let frames = self
            .frames
            .iter()
            .step_by(step)
            .map(|frame| {
                let mut s = String::with_capacity(frame.len() * 6);
                for [r, g, b] in frame {
                    write!(s, "{r:02x}{g:02x}{b:02x}").unwrap();
                }
                s
            })
            .collect();
        (ms, frames)
    }

    pub fn timing(&self) -> crate::types::RgbPlaybackTiming {
        let secondary = self.secondary;
        crate::types::RgbPlaybackTiming {
            interval_hundredths: self.interval_hundredths,
            secondary_interval_ticks: secondary.map_or(0, |s| s.interval_ticks),
            secondary_frame_count: secondary.map_or(0, |s| s.frame_count),
            outer_longest: secondary.is_some_and(|s| s.outer_longest),
        }
    }
}
