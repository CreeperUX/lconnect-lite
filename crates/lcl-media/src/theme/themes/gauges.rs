//! 06 · 仪表组：上方 20–50 °C 半圆水温表（强调色），下方 CPU / GPU 两个小圆环，底部一行泵和风扇读数。

use crate::theme::{polar, rgb, Canvas, Rgba, Style, TextStyle, ThemeData};

const CX: f32 = 240.0;
const CY: f32 = 240.0;
const R: f32 = 170.0;

struct Palette {
    bg: Rgba,
    fg: Rgba,
    muted: Rgba,
    faint: Rgba,
    track: Rgba,
    cpu: Rgba,
    gpu: Rgba,
    pump: Rgba,
    fan: Rgba,
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette {
            bg: rgb(0x0b0c0e),
            fg: rgb(0xecebe6),
            muted: rgb(0x8c939b),
            faint: rgb(0x5b6168),
            track: rgb(0x1c2126),
            cpu: rgb(0xb9a7e8),
            gpu: rgb(0xe58fb6),
            pump: rgb(0x6fb3e0),
            fan: rgb(0x5bc08a),
        }
    } else {
        Palette {
            bg: rgb(0xf4f5f7),
            fg: rgb(0x14171b),
            muted: rgb(0x6b727a),
            faint: rgb(0x9aa0a7),
            track: rgb(0xe1e4e8),
            cpu: rgb(0x8a72d6),
            gpu: rgb(0xd0628f),
            pump: rgb(0x3a8cc4),
            fan: rgb(0x2f9d66),
        }
    }
}

/// 一行混排文字（每段自己的字体和颜色），整体以 `cx` 居中，`y` 是基线。
fn runs_centered(c: &mut Canvas, parts: &[(&str, TextStyle, Rgba)], cx: f32, y: f32) {
    let total: f32 = parts.iter().map(|(t, st, _)| c.measure(t, st)).sum();
    runs_at(c, parts, cx - total / 2.0, y);
}

/// 从 `x` 开始依次写各段，返回总宽度。
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

/// 读数缺失（“—”）时不带单位。
fn unit<'a>(value: &str, u: &'a str) -> &'a str {
    if value == "—" {
        ""
    } else {
        u
    }
}

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.bg);

    // 上半圆水温表：从左（180°）顺时针扫 180° 到右，直头。
    c.stroke_arc(CX, CY, R, 180.0, 180.0, 22.0, false, p.track);
    if d.coolant_c.is_some() {
        c.stroke_arc(CX, CY, R, 180.0, 180.0 * d.coolant_frac(), 22.0, false, s.accent);
    }
    // 五根背景色的分隔刻度把弧切成四段。
    for i in 0..5 {
        let deg = 180.0 + i as f32 * 45.0;
        let (a, b) = (polar(CX, CY, 182.0, deg), polar(CX, CY, 158.0, deg));
        c.stroke_line(a.0, a.1, b.0, b.1, 3.0, false, p.bg);
    }
    let scale = TextStyle::num(13.0, 400).center();
    c.text("20", 70.0, 268.0, &scale, p.faint);
    c.text("50", 410.0, 268.0, &scale, p.faint);

    // CPU / GPU 小圆环：从正上方顺时针。
    let rings = [(140.0, d.has_cpu().then(|| d.cpu_frac()), p.cpu), (340.0, d.gpu_temp_c.map(|_| d.gpu_frac()), p.gpu)];
    for (x, frac, col) in rings {
        c.stroke_circle(x, 350.0, 54.0, 10.0, p.track);
        if let Some(f) = frac {
            c.stroke_arc(x, 350.0, 54.0, 270.0, 360.0 * f, 10.0, true, col);
        }
    }

    // 中间大读数。
    c.text("水温 °C", CX, 134.5, &TextStyle::ui(18.0).center(), p.muted);
    c.text(&d.coolant(), CX, 216.0, &TextStyle::num(96.0, 400).center(), p.fg);

    // 小圆环里的读数和标签。
    let (cpu, gpu) = (d.cpu(), d.gpu());
    let big = TextStyle::num(28.0, 500);
    let small = TextStyle::num(16.0, 500);
    runs_centered(c, &[(&cpu, big, p.fg), (unit(&cpu, d.cpu_unit()), small, p.muted)], 140.0, 349.5);
    runs_centered(c, &[(&gpu, big, p.fg), (unit(&gpu, "°C"), small, p.muted)], 340.0, 349.5);
    c.text("CPU", 140.0, 370.0, &TextStyle::ui(13.0).center(), p.cpu);
    c.text("GPU", 340.0, 370.0, &TextStyle::ui(13.0).center(), p.gpu);

    // 底部：泵和风扇，两组之间空 22px，整体居中。
    let (pump, fan) = (d.pump(), d.fan());
    let ui = TextStyle::ui(16.0);
    let num = TextStyle::num(16.0, 400);
    let g1 = [("泵", ui, p.pump), (" ", ui, p.muted), (pump.as_str(), num, p.fg), (if pump == "—" { "" } else { " rpm" }, ui, p.muted)];
    let g2 = [("风扇", ui, p.fan), (" ", ui, p.muted), (fan.as_str(), num, p.fg), (unit(&fan, "%"), ui, p.muted)];
    let (w1, w2) = (runs_width(c, &g1), runs_width(c, &g2));
    let x = CX - (w1 + 22.0 + w2) / 2.0;
    runs_at(c, &g1, x, 453.0);
    runs_at(c, &g2, x + w1 + 22.0, 453.0);
}
