//! 14 · 液面：圆形玻璃缸，水位随水温升降（前后两层波浪、气泡），玻璃高光和外圈柔光；强调色是水面的颜色。

use crate::theme::{alpha, arc_path, rgb, rgba, Canvas, Paint, Rgba, Style, TextStyle, ThemeData};
use tiny_skia::{Path, PathBuilder};

const CX: f32 = 240.0;
const CY: f32 = 226.0;
const R: f32 = 164.0;
/// 缸底。
const FLOOR: f32 = CY + R;
/// 设计稿里 27 °C 的水位线。
const DESIGN_LEVEL: f32 = 270.0;

struct Palette {
    bg: Paint,
    deep: Rgba,
    fg: Rgba,
    muted: Rgba,
    label: Rgba,
    bowl: Rgba,
    rim: Rgba,
    /// 玻璃反光径向渐变三个位置的白色透明度。
    glass: [f32; 3],
    shine: f32,
    halo: (Rgba, f32),
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette {
            bg: Paint::radial((240.0, 192.0), 375.0, &[(0.0, rgb(0x0f1a24)), (0.7, rgb(0x05080b))]),
            deep: rgb(0x0a2a6b),
            fg: rgb(0xf2f7fa),
            muted: rgba(0xe6f5ff, 0.7),
            label: rgb(0x8fa3b0),
            bowl: rgb(0x071018),
            rim: rgba(0xffffff, 0.16),
            glass: [0.1, 0.02, 0.06],
            shine: 0.35,
            halo: (s.accent, 0.18),
        }
    } else {
        Palette {
            bg: Paint::radial((240.0, 192.0), 375.0, &[(0.0, rgb(0xffffff)), (0.6, rgb(0xeaf2f8)), (1.0, rgb(0xd8e5ef))]),
            deep: rgb(0x1764c8),
            fg: rgb(0x10202c),
            muted: rgba(0x10202c, 0.62),
            label: rgb(0x5f7584),
            bowl: rgb(0xf6fbfe),
            rim: rgba(0x7896af, 0.45),
            glass: [0.7, 0.15, 0.25],
            shine: 0.9,
            halo: (rgb(0x5a7f9c), 0.25),
        }
    }
}

/// 水温比例 → 水位线的 y。线性映射：27 °C 正好落在设计稿的 270，50 °C 接近满缸，20 °C 时还有约两成水。
fn level_y(frac: f32) -> f32 {
    let design = (FLOOR - DESIGN_LEVEL) / (2.0 * R); // 设计稿水位占缸高的比例（≈0.366）
    let at27 = 7.0 / 30.0;
    // 分两段线性：20 °C 时 12%（波峰低于泵速那行字），27 °C 是设计稿，50 °C 时 92%。
    let fill = if frac < at27 { 0.12 + frac / at27 * (design - 0.12) } else { design + (frac - at27) * (0.92 - design) / (1.0 - at27) };
    FLOOR - fill * 2.0 * R
}

/// 前面一层波：起点 (60, 0) 之后三段三次贝塞尔，每个点是 (x, 相对水位的 dy)。
/// 设计稿里的 `S` 段已经按对称把第一个控制点展开。
const FRONT: [[(f32, f32); 3]; 3] = [
    [(100.0, 14.0), (150.0, -16.0), (200.0, 0.0)],
    [(250.0, 16.0), (280.0, 16.0), (330.0, 0.0)],
    [(380.0, -16.0), (400.0, -16.0), (440.0, 0.0)],
];
/// 后面一层波（相位相反）。
const BACK: [[(f32, f32); 3]; 3] = [
    [(110.0, -16.0), (150.0, 16.0), (200.0, 0.0)],
    [(250.0, -16.0), (290.0, -16.0), (340.0, 0.0)],
    [(390.0, 16.0), (410.0, 16.0), (440.0, 0.0)],
];

/// 波浪线；`close` 时往下封口成水体。
fn wave(level: f32, segs: &[[(f32, f32); 3]; 3], close: bool) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(60.0, level);
    for [a, b, e] in segs {
        pb.cubic_to(a.0, level + a.1, b.0, level + b.1, e.0, level + e.1);
    }
    if close {
        pb.line_to(440.0, 400.0);
        pb.line_to(60.0, 400.0);
        pb.close();
    }
    pb.finish()
}

/// 一行混排文字，从 `x` 开始，返回总宽度。
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
    c.fill_all(p.bg.clone());

    // 外圈柔光（feGaussianBlur stdDeviation 14），比缸略往下偏。
    let (halo, halo_a) = p.halo;
    c.glow(14.0, halo_a, |l| l.fill_circle(CX, CY + 10.0, 168.0, halo));
    c.fill_circle(CX, CY, R, p.bowl);

    // 水：没有水温读数时缸是空的。
    let level = d.coolant_c.map(|_| level_y(d.coolant_frac()));
    if let (Some(level), Some(clip)) = (level, PathBuilder::from_circle(CX, CY, R)) {
        let top = s.accent;
        let deep = p.deep;
        c.clipped(&clip, |l| {
            // 后面一层浅一些、相位相反的波。
            let back = level - 8.0;
            if let Some(path) = wave(back, &BACK, true) {
                let paint = Paint::linear((0.0, back - 10.0), (0.0, 400.0), &[(0.0, alpha(top, 0.55)), (1.0, alpha(deep, 0.55))]);
                l.fill_path(&path, paint);
            }
            let water = Paint::linear((0.0, level - 10.0), (0.0, 400.0), &[(0.0, top), (1.0, deep)]);
            if let Some(path) = wave(level, &FRONT, true) {
                l.fill_path(&path, water);
            }
            if let Some(path) = wave(level, &FRONT, false) {
                l.stroke_path(&path, 2.0, false, rgba(0xe8fbff, 0.6));
            }
            // 气泡：按设计稿在水里的相对深度摆放，水位变了也留在水里。
            let depth = |y: f32| level + (y - DESIGN_LEVEL) / (FLOOR - DESIGN_LEVEL) * (FLOOR - level);
            for (x, y, r, a) in [(180.0, 340.0, 4.0, 0.35), (196.0, 312.0, 2.5, 0.45), (300.0, 352.0, 5.0, 0.25), (286.0, 304.0, 2.0, 0.5), (246.0, 372.0, 3.0, 0.3)] {
                l.fill_circle(x, depth(y), r, rgba(0xe8fbff, a));
            }
        });
    }

    // 玻璃：左上偏亮的径向反光、细边、左上一道高光弧。
    let [g1, g2, g3] = p.glass;
    let glass = Paint::radial((190.8, 144.0), 262.4, &[(0.0, rgba(0xffffff, g1)), (0.6, rgba(0xffffff, g2)), (1.0, rgba(0xffffff, g3))]);
    c.fill_circle(CX, CY, R, glass);
    c.stroke_circle(CX, CY, 163.0, 2.0, p.rim);
    if let Some(arc) = arc_path(CX, CY, 150.0, 219.0, 38.6) {
        c.stroke_path(&arc, 6.0, true, rgba(0xffffff, p.shine));
    }

    // ---- 文字 ----
    c.text("水温", CX, 128.5, &TextStyle::ui(16.0).center().spacing_em(0.3), p.muted);
    let value = d.coolant();
    let big = TextStyle::num(104.0, 300);
    let unit = TextStyle::num(30.0, 300);
    let parts = [(value.as_str(), big, p.fg), (if d.coolant_c.is_some() { "°C" } else { "" }, unit, p.muted)];
    let w = runs_width(c, &parts);
    runs_at(c, &parts, CX - w / 2.0, 222.5);

    // 泵速：水里的部分用近白色，水面以上的部分用弱化色——水位低时这行字会跨过水面。
    let pump = d.pump();
    let pump_text = if d.pump_rpm.is_some() { format!("泵 {pump} rpm") } else { format!("泵 {pump}") };
    let pump_style = TextStyle::num(18.0, 400).center();
    match level.and_then(|l| wave(l, &FRONT, true)) {
        Some(water) => {
            // 画布矩形逆时针 + 水体顺时针，非零环绕规则下水体变成洞，得到“水面以上”。
            let mut pb = PathBuilder::new();
            pb.move_to(0.0, 0.0);
            pb.line_to(0.0, 480.0);
            pb.line_to(480.0, 480.0);
            pb.line_to(480.0, 0.0);
            pb.close();
            pb.push_path(&water);
            if let Some(air) = pb.finish() {
                c.clipped(&air, |l| {
                    l.text(&pump_text, CX, 318.0, &pump_style, p.muted);
                });
            }
            c.clipped(&water, |l| {
                l.text(&pump_text, CX, 318.0, &pump_style, rgba(0xf0faff, 0.92));
            });
        }
        None => {
            c.text(&pump_text, CX, 318.0, &pump_style, p.muted);
        }
    }

    // 底部三组：风扇 / CPU / GPU，左右各留 40px，两端对齐。
    let (fan, cpu, gpu) = (d.fan(), d.cpu(), d.gpu());
    let fan = if d.fan_percent.is_some() { format!("{fan}%") } else { fan };
    let cpu = if d.has_cpu() { format!("{cpu}{}", d.cpu_unit()) } else { cpu };
    let gpu = if d.gpu_temp_c.is_some() { format!("{gpu}°C") } else { gpu };
    let lab = TextStyle::ui(15.0);
    let num = TextStyle::num(20.0, 500);
    let groups = [
        [("风扇 ", lab, p.label), (fan.as_str(), num, p.fg)],
        [("CPU ", lab, p.label), (cpu.as_str(), num, p.fg)],
        [("GPU ", lab, p.label), (gpu.as_str(), num, p.fg)],
    ];
    let widths: Vec<f32> = groups.iter().map(|g| runs_width(c, g)).collect();
    let gap = (400.0 - widths.iter().sum::<f32>()) / 2.0;
    let mut x = 40.0;
    for (g, w) in groups.iter().zip(&widths) {
        runs_at(c, g, x, 452.0);
        x += w + gap;
    }
}
