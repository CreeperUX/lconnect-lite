//! 冷头屏幕主题：15 套，其中 11 套有暗色 / 亮色两种模式。
//! 设计稿在“LConnect Lite 界面候选”画布最下面三行；每套一个文件，坐标直接用设计稿上的像素（480×480）。
//!
//! 新主题：在 `themes/` 里加一个文件，实现 `render(c, d, s)`，再在 [`THEMES`] 里登记。

pub mod canvas;
pub mod fonts;
mod themes;

pub use canvas::{alpha, arc_path, hash, mix, polar, rgb, rgba, round_rect, Align, Canvas, Paint, Rgba, TextStyle};
pub use fonts::Family;

use image::RgbImage;

pub const SIZE: u32 = 480;

/// 画面上要显示的数据。没有读数的项为 None，主题显示“—”。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeData {
    pub coolant_c: Option<f32>,
    /// 最近两分钟水温变化（°C），用来显示“稳定 / 上升 / 下降”。
    pub coolant_trend: Option<f32>,
    /// 最近十分钟的水温，旧的在前，大约每 10 秒一个点；走势线用。
    pub coolant_history: Vec<f32>,
    pub pump_rpm: Option<u16>,
    pub pump_target_rpm: Option<u16>,
    pub fan_percent: Option<f32>,
    /// 每个风扇的转速。
    pub fan_rpm: Vec<u16>,
    pub cpu_usage: Option<f32>,
    /// CPU 封装温度（传感器服务在运行时才有）。有温度时主题里的 CPU 显示温度，没有时退回占用率。
    pub cpu_temp_c: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub gpu_usage: Option<f32>,
    /// 本地时间（时, 分）。
    pub time: (u8, u8),
    /// 月、日、星期（0 = 星期日）。
    pub date: (u8, u8, u8),
}

impl ThemeData {
    /// 设计稿上用的示例数据，预览和测试用。
    pub fn sample() -> Self {
        Self {
            coolant_c: Some(27.0),
            coolant_trend: Some(0.1),
            coolant_history: vec![26.4, 26.4, 26.5, 26.5, 26.6, 26.5, 26.7, 26.7, 26.8, 26.9, 26.8, 27.0, 27.0],
            pump_rpm: Some(2382),
            pump_target_rpm: Some(2400),
            fan_percent: Some(36.0),
            fan_rpm: vec![890, 892, 895],
            cpu_usage: Some(14.0),
            cpu_temp_c: Some(46.0),
            gpu_temp_c: Some(30.0),
            gpu_usage: Some(8.0),
            time: (14, 35),
            date: (10, 21, 3),
        }
    }

    /// 画面上实际显示的内容；它不变就不用重新渲染和推送。
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{:02}:{:02}|{:?}|{:?}",
            self.coolant(),
            self.pump(),
            self.fan(),
            self.cpu(),
            self.gpu(),
            self.trend_label().0,
            self.fan_rpm.len(),
            self.time.0,
            self.time.1,
            self.date,
            // 走势线用到的历史只取一位小数，避免每秒都变。
            self.coolant_history.iter().map(|v| (v * 10.0).round() as i32).collect::<Vec<_>>()
        )
    }

    pub fn coolant(&self) -> String {
        opt(self.coolant_c, 1)
    }
    pub fn pump(&self) -> String {
        self.pump_rpm.map_or_else(dash, |v| v.to_string())
    }
    pub fn fan(&self) -> String {
        opt(self.fan_percent, 0)
    }
    /// CPU 读数：有温度显示温度，否则显示占用率。单位用 [`Self::cpu_unit`]。
    pub fn cpu(&self) -> String {
        opt(self.cpu_temp_c.or(self.cpu_usage), 0)
    }
    /// CPU 读数的单位：温度“°C”，占用率“%”。
    pub fn cpu_unit(&self) -> &'static str {
        if self.cpu_temp_c.is_some() {
            "°C"
        } else {
            "%"
        }
    }
    pub fn has_cpu(&self) -> bool {
        self.cpu_temp_c.is_some() || self.cpu_usage.is_some()
    }
    pub fn gpu(&self) -> String {
        opt(self.gpu_temp_c, 0)
    }
    pub fn clock(&self) -> String {
        format!("{:02}:{:02}", self.time.0, self.time.1)
    }
    /// “10月21日 星期三”
    pub fn date_cn(&self) -> String {
        const WEEK: [&str; 7] = ["日", "一", "二", "三", "四", "五", "六"];
        format!("{}月{}日 星期{}", self.date.0, self.date.1, WEEK[usize::from(self.date.2 % 7)])
    }

    /// 水温在 20–50 °C 量程里的比例（0–1）。
    pub fn coolant_frac(&self) -> f32 {
        frac(self.coolant_c, 20.0, 50.0)
    }
    /// 泵速在 1600–3200 rpm（方屏冷头的可调范围）里的比例，最低转速是空条。
    pub fn pump_frac(&self) -> f32 {
        frac(self.pump_rpm.map(f32::from), 1600.0, 3200.0)
    }
    pub fn fan_frac(&self) -> f32 {
        frac(self.fan_percent, 0.0, 100.0)
    }
    /// CPU 温度按 0–100 °C、占用率按 0–100% 算比例。
    pub fn cpu_frac(&self) -> f32 {
        frac(self.cpu_temp_c.or(self.cpu_usage), 0.0, 100.0)
    }
    /// GPU 温度按 0–100 °C 算比例（30 °C 就是 30%，和设计稿一致）。
    pub fn gpu_frac(&self) -> f32 {
        frac(self.gpu_temp_c, 0.0, 100.0)
    }

    /// 水温趋势：（文字，-1 下降 / 0 稳定 / 1 上升）。
    pub fn trend_label(&self) -> (&'static str, i8) {
        match self.coolant_trend {
            Some(t) if t >= 0.3 => ("上升", 1),
            Some(t) if t <= -0.3 => ("下降", -1),
            _ => ("稳定", 0),
        }
    }
}

fn dash() -> String {
    "—".into()
}

fn opt(v: Option<f32>, digits: usize) -> String {
    v.map_or_else(dash, |v| format!("{v:.digits$}"))
}

fn frac(v: Option<f32>, lo: f32, hi: f32) -> f32 {
    v.map_or(0.0, |v| ((v - lo) / (hi - lo)).clamp(0.0, 1.0))
}

/// 主题外观参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// 暗色模式；只有暗色的主题忽略它。
    pub dark: bool,
    /// 强调色。
    pub accent: Rgba,
}

/// 设计稿分组：扁平、适中、华丽。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Flat,
    Medium,
    Fancy,
}

pub struct ThemeInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub tier: Tier,
    /// 有亮色模式。
    pub light: bool,
    /// 默认强调色（0xRRGGBB）。
    pub accent: u32,
    pub render: fn(&mut Canvas, &ThemeData, &Style),
}

/// 全部主题，按设计稿顺序（从扁平到华丽）。
pub static THEMES: &[ThemeInfo] = &[
    ThemeInfo { id: "swiss", name: "瑞士网格", tier: Tier::Flat, light: true, accent: 0xe8402c, render: themes::swiss::render },
    ThemeInfo { id: "line", name: "单线", tier: Tier::Flat, light: true, accent: 0xf2a33a, render: themes::line::render },
    ThemeInfo { id: "block", name: "色块海报", tier: Tier::Flat, light: true, accent: 0xf2a33a, render: themes::block::render },
    ThemeInfo { id: "tiles", name: "数据卡片", tier: Tier::Flat, light: true, accent: 0xf2a33a, render: themes::tiles::render },
    ThemeInfo { id: "minimal", name: "极简大字", tier: Tier::Flat, light: true, accent: 0xf2a33a, render: themes::minimal::render },
    ThemeInfo { id: "gauges", name: "仪表组", tier: Tier::Medium, light: true, accent: 0xf2a33a, render: themes::gauges::render },
    ThemeInfo { id: "clock", name: "时钟", tier: Tier::Medium, light: true, accent: 0xf2a33a, render: themes::clock::render },
    ThemeInfo { id: "retro", name: "复古液晶", tier: Tier::Medium, light: false, accent: 0x7dff9a, render: themes::retro::render },
    ThemeInfo { id: "bars", name: "柱状条", tier: Tier::Medium, light: true, accent: 0xf2a33a, render: themes::bars::render },
    ThemeInfo { id: "ring", name: "光环表盘", tier: Tier::Fancy, light: true, accent: 0xf2a33a, render: themes::ring::render },
    ThemeInfo { id: "glass", name: "极光玻璃", tier: Tier::Fancy, light: true, accent: 0xff8a3d, render: themes::glass::render },
    ThemeInfo { id: "infinity", name: "无限镜", tier: Tier::Fancy, light: false, accent: 0xff7a2f, render: themes::infinity::render },
    ThemeInfo { id: "neon", name: "霓虹", tier: Tier::Fancy, light: false, accent: 0xc03cff, render: themes::neon::render },
    ThemeInfo { id: "liquid", name: "液面", tier: Tier::Fancy, light: true, accent: 0x3fd0ff, render: themes::liquid::render },
    ThemeInfo { id: "radar", name: "雷达", tier: Tier::Fancy, light: false, accent: 0x3cffb4, render: themes::radar::render },
];

/// 按 id 找主题，找不到用光环表盘。
pub fn find(id: &str) -> &'static ThemeInfo {
    THEMES.iter().find(|t| t.id == id).unwrap_or(&THEMES[9])
}

/// 渲染一帧 480×480。只有暗色的主题在亮色模式下照样画暗色。
pub fn render(id: &str, data: &ThemeData, style: &Style) -> RgbImage {
    let theme = find(id);
    let style = Style { dark: style.dark || !theme.light, ..*style };
    let mut c = Canvas::new(SIZE, SIZE);
    (theme.render)(&mut c, data, &style);
    c.into_rgb()
}

/// 还没实现的主题先画一个占位。
#[allow(dead_code)]
pub(crate) fn placeholder(c: &mut Canvas, name: &str, s: &Style) {
    c.fill_all(if s.dark { rgb(0x0b0c0e) } else { rgb(0xf4f5f7) });
    let fg = if s.dark { rgb(0x8c939b) } else { rgb(0x6b727a) };
    c.text(name, 240.0, 230.0, &TextStyle::ui(28.0).center(), fg);
    c.text("待实现", 240.0, 270.0, &TextStyle::ui(16.0).center(), fg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_renders_both_modes() {
        let d = ThemeData::sample();
        for t in THEMES {
            for dark in [true, false] {
                let img = render(t.id, &d, &Style { dark, accent: rgb(t.accent) });
                assert_eq!(img.dimensions(), (SIZE, SIZE), "{}", t.id);
            }
        }
    }

    #[test]
    fn missing_readings_show_dash() {
        let d = ThemeData::default();
        assert_eq!(d.coolant(), "—");
        // 没有读数也要能画出来，不能 panic。
        for t in THEMES {
            render(t.id, &d, &Style { dark: true, accent: rgb(t.accent) });
        }
    }

    #[test]
    fn key_ignores_sub_display_changes() {
        let mut a = ThemeData::sample();
        let mut b = ThemeData::sample();
        a.coolant_c = Some(27.01);
        b.coolant_c = Some(27.04);
        assert_eq!(a.key(), b.key());
        b.time.1 += 1;
        assert_ne!(a.key(), b.key());
    }
}
