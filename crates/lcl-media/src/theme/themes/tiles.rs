//! 04 · 数据卡片：2×2 圆角卡片——水温带走势线、泵速带进度条、风扇每个一根竖条、CPU / GPU 两条横条。

use crate::theme::{alpha, rgb, Canvas, Rgba, Style, TextStyle, ThemeData};

struct Palette {
    bg: Rgba,
    fg: Rgba,
    muted: Rgba,
    track: Rgba,
    card: Rgba,
    /// 亮色模式卡片有一层淡淡的投影。
    shadow: bool,
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette { bg: rgb(0x08090a), fg: rgb(0xecebe6), muted: rgb(0x8c939b), track: rgb(0x23272c), card: rgb(0x15181c), shadow: false }
    } else {
        Palette { bg: rgb(0xe9ecf0), fg: rgb(0x15181c), muted: rgb(0x6c737b), track: rgb(0xe4e7eb), card: rgb(0xffffff), shadow: true }
    }
}

const PUMP: Rgba = rgb(0x4f9fd6);
const FAN: Rgba = rgb(0x3fb57a);
const CPU: Rgba = rgb(0x9a86d8);
const GPU: Rgba = rgb(0xd6739f);

/// 卡片 216×216，外边距 18，间距 12。
const CARD: f32 = 216.0;
const A: f32 = 18.0;
const B: f32 = 246.0;

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.bg);

    let cards = [(A, A), (B, A), (A, B), (B, B)];
    if p.shadow {
        // box-shadow: 0 1px 2px / 0 8px 20px，都是 rgba(20,30,40,0.06)。四张一起模糊，省时间。
        c.glow(1.0, 0.06, |l| cards.iter().for_each(|&(x, y)| l.fill_round_rect(x, y + 1.0, CARD, CARD, 22.0, rgb(0x141e28))));
        c.glow(10.0, 0.06, |l| cards.iter().for_each(|&(x, y)| l.fill_round_rect(x, y + 8.0, CARD, CARD, 22.0, rgb(0x141e28))));
    }
    for &(x, y) in &cards {
        c.fill_round_rect(x, y, CARD, CARD, 22.0, p.card);
    }

    let label = TextStyle::ui(17.0);
    let unit24 = TextStyle::num(24.0, 400);

    // ① 水温：大数字 + “°”，底部走势线。
    c.text("水温", 36.0, 53.0, &label, p.muted);
    let w = c.text( &d.coolant(), 36.0, 133.5, &TextStyle::num(64.0, 400), p.fg);
    c.text( "°", 36.0 + w, 133.5, &unit24, p.muted);
    sparkline(c, d, s.accent, 36.0, 172.0, 186.0, 44.0);

    // ② 泵速：数字 + 进度条 + 目标转速。
    c.text("泵速", 264.0, 53.0, &label, p.muted);
    c.text( &d.pump(), 264.0, 134.5, &TextStyle::num(54.0, 400), p.fg);
    bar(c, 264.0, 180.0, 180.0, 10.0, d.pump_frac(), p.track, PUMP);
    let target = d.pump_target_rpm.map_or_else(|| "rpm".to_string(), |t| format!("rpm · 目标 {t}"));
    c.text(&target, 264.0, 212.0, &TextStyle::ui(14.0), p.muted);

    // ③ 风扇：百分比 + 每个风扇一根竖条。
    c.text("风扇", 36.0, 281.0, &label, p.muted);
    let w = c.text( &d.fan(), 36.0, 360.5, &TextStyle::num(54.0, 400), p.fg);
    c.text( "%", 36.0 + w, 360.5, &unit24, p.muted);
    fan_bars(c, d, 36.0, 404.0, 180.0, 40.0, p.track);

    // ④ CPU 占用和 GPU 温度：标签左、数字右对齐，下面一条细横条。
    let rows = [("CPU", d.cpu(), d.cpu_unit(), d.cpu_frac(), CPU, 291.0), ("GPU", d.gpu(), "°C", d.gpu_frac(), GPU, 423.0)];
    let num = TextStyle::num(34.0, 400).right();
    let unit = TextStyle::num(18.0, 400).right();
    for (name, v, u, frac, col, base) in rows {
        c.text(name, 264.0, base, &label, col);
        let uw = c.text( u, 444.0, base, &unit, p.muted);
        c.text( &v, 444.0 - uw, base, &num, p.fg);
        bar(c, 264.0, base + 13.0, 180.0, 8.0, frac, p.track, col);
    }
}

/// 圆角横条：底槽 + 按比例的填充。
#[allow(clippy::too_many_arguments)]
fn bar(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, frac: f32, track: Rgba, fill: Rgba) {
    c.fill_round_rect(x, y, w, h, h / 2.0, track);
    if frac > 0.0 {
        c.fill_round_rect(x, y, (w * frac).max(h), h, h / 2.0, fill);
    }
}

/// 风扇竖条：有几个风扇画几根（没有转速列表时画 3 根）。高度是总风扇占空比，
/// 再按各自转速相对平均值微调，看得出哪个转得快。
fn fan_bars(c: &mut Canvas, d: &ThemeData, x: f32, y: f32, w: f32, h: f32, track: Rgba) {
    let n = if d.fan_rpm.is_empty() { 3 } else { d.fan_rpm.len().min(6) };
    let rpms = &d.fan_rpm[..d.fan_rpm.len().min(n)];
    let mean = rpms.iter().map(|&r| f32::from(r)).sum::<f32>() / rpms.len().max(1) as f32;
    let gap = 6.0;
    let bw = (w - gap * (n as f32 - 1.0)) / n as f32;
    let base = d.fan_frac();
    for i in 0..n {
        let bx = x + i as f32 * (bw + gap);
        c.fill_round_rect(bx, y, bw, h, 6.0, track);
        let frac = match rpms.get(i) {
            Some(&r) if mean > 0.0 => (base * f32::from(r) / mean).clamp(0.0, 1.0),
            _ => base,
        };
        if frac > 0.0 {
            let fh = h * frac;
            c.fill_round_rect(bx, y + h - fh, bw, fh, 6.0, FAN);
        }
    }
}

/// 水温走势：最近十分钟的历史铺满宽度，量程至少 0.8 °C（避免小抖动被放大成大起伏）。
fn sparkline(c: &mut Canvas, d: &ThemeData, accent: Rgba, x: f32, y: f32, w: f32, h: f32) {
    let hist = &d.coolant_history;
    if hist.is_empty() {
        return;
    }
    let (mut lo, mut hi) = hist.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    if hi - lo < 0.8 {
        let mid = (hi + lo) / 2.0;
        (lo, hi) = (mid - 0.4, mid + 0.4);
    }
    // 设计稿里线在 y=20–31 之间（svg 高 44），底下留空给填充。
    let (top, bottom) = (y + 19.0, y + 32.0);
    let step = if hist.len() > 1 { w / (hist.len() - 1) as f32 } else { 0.0 };
    let mut pts: Vec<(f32, f32)> = hist.iter().enumerate().map(|(i, &v)| (x + i as f32 * step, bottom - (v - lo) / (hi - lo) * (bottom - top))).collect();
    if pts.len() == 1 {
        pts.push((x + w, pts[0].1));
    }
    let mut area = pts.clone();
    area.push((x + w, y + h));
    area.push((x, y + h));
    c.fill_polygon(&area, alpha(accent, 0.18));
    c.stroke_polyline(&pts, 2.5, false, accent);
}
