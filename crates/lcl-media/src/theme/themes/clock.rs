//! 07 · 时钟：上下两道强调色折角框（带淡光晕），中间细体大时间、日期，底部一行水温和泵速。

use crate::theme::{alpha, rgb, Canvas, Rgba, Style, TextStyle, ThemeData};

struct Palette {
    bg: Rgba,
    fg: Rgba,
    muted: Rgba,
    /// 折角框外面那圈粗描边的透明度。
    halo: f32,
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette { bg: rgb(0x0a0b0d), fg: rgb(0xecebe6), muted: rgb(0x8c939b), halo: 0.12 }
    } else {
        Palette { bg: rgb(0xf7f5f0), fg: rgb(0x15130f), muted: rgb(0x857f74), halo: 0.18 }
    }
}

/// 从 `x` 开始依次写各段（每段自己的字体和颜色），返回总宽度。
fn runs_at(c: &mut Canvas, parts: &[(&str, TextStyle, Rgba)], x: f32, y: f32) -> f32 {
    let mut x0 = x;
    for (t, st, col) in parts {
        x0 += c.text(t, x0, y, st, *col);
    }
    x0 - x
}

fn runs_width(c: &Canvas, parts: &[(&str, TextStyle, Rgba)]) -> f32 {
    parts.iter().map(|(t, st, _)| c.measure(t, st)).sum()
}

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.bg);

    // 上下两道折角框：先画 22px 宽的淡色光晕，再画 8px 的实线。
    let top = [(72.0, 76.0), (108.0, 40.0), (372.0, 40.0), (408.0, 76.0)];
    let bottom = top.map(|(x, y)| (x, 480.0 - y));
    for pts in [&top, &bottom] {
        c.stroke_polyline(pts, 22.0, true, alpha(s.accent, p.halo));
        c.stroke_polyline(pts, 8.0, true, s.accent);
    }

    // 时间和日期。
    c.text(&d.clock(), 240.0, 223.0, &TextStyle::num(132.0, 300).center().spacing_em(-0.01), p.fg);
    c.text(&d.date_cn(), 240.0, 284.5, &TextStyle::ui(22.0).center(), p.muted);

    // 底部：左边水温、右边泵速，基线对齐，左右各留 96px。
    let (coolant, pump) = (d.coolant(), d.pump());
    let label = TextStyle::ui(17.0);
    let num = TextStyle::num(30.0, 500);
    let unit = if d.coolant_c.is_some() { "°C" } else { "" };
    let left = [("水温 ", label, p.muted), (coolant.as_str(), num, p.fg), (unit, TextStyle::ui(16.0), p.muted)];
    let right = [("泵 ", label, p.muted), (pump.as_str(), num, p.fg)];
    let base = 349.0;
    runs_at(c, &left, 96.0, base);
    let wr = runs_width(c, &right);
    runs_at(c, &right, 384.0 - wr, base);
}
