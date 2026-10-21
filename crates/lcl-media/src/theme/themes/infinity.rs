//! 12 · 无限镜：上下两条切角灯带向中心一层层缩小、变暗，叠出镜面纵深；主灯带带辉光和亮芯，中间是大号时间。

use crate::theme::{alpha, rgb, Canvas, Paint, Rgba, Style, TextStyle, ThemeData};
use tiny_skia::{Path, PathBuilder};

/// 灯带：上下两段切角折线（设计稿 SVG 坐标）。
const STRIP: [[(f32, f32); 4]; 2] = [
    [(52.0, 92.0), (100.0, 44.0), (380.0, 44.0), (428.0, 92.0)],
    [(52.0, 388.0), (100.0, 436.0), (380.0, 436.0), (428.0, 388.0)],
];
const FG: u32 = 0xf4f2ec;
const LABEL: u32 = 0x9aa1a8;
/// 渐变中段的暖白。
const ACCENT2: u32 = 0xffd08a;

/// 以画面中心缩放后的灯带路径。
fn strip(sx: f32, sy: f32) -> Option<Path> {
    let t = |(x, y): (f32, f32)| (240.0 + (x - 240.0) * sx, 240.0 + (y - 240.0) * sy);
    let mut pb = PathBuilder::new();
    for seg in &STRIP {
        let (x, y) = t(seg[0]);
        pb.move_to(x, y);
        for &pt in &seg[1..] {
            let (x, y) = t(pt);
            pb.line_to(x, y);
        }
    }
    pb.finish()
}

/// 灯带的横向渐变（强调色 → 暖白 → 强调色），跟着缩放。
fn strip_paint(accent: Rgba, sx: f32, a: f32) -> Paint {
    let x = |v: f32| 240.0 + (v - 240.0) * sx;
    let (c1, c2) = (alpha(accent, a), alpha(rgb(ACCENT2), a));
    Paint::linear((x(52.0), 0.0), (x(428.0), 0.0), &[(0.0, c1), (0.5, c2), (1.0, c1)])
}

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    // 背景：中心略亮的径向渐变（ellipse farthest-corner，正方形里就是半径 339 的圆）。
    c.fill_all(Paint::radial((240.0, 240.0), 339.4, &[(0.0, rgb(0x0b0d10)), (0.8, rgb(0x020203)), (1.0, rgb(0x020203))]));

    // 镜面里的倒影：6 层，每层缩到 0.9 倍、纵向再稍微压扁，透明度按 0.6 递减；线宽保持约 6px。
    for i in 1..=6 {
        let k = 0.9f32.powi(i);
        let (sx, sy) = (k, 0.9 * k + 0.1 * 0.8f32.powi(i));
        let o = 0.55 * 0.6f32.powi(i - 1);
        let w = 6.0 / k * (sx + sy) / 2.0;
        if let Some(path) = strip(sx, sy) {
            c.stroke_path(&path, w, true, strip_paint(s.accent, sx, o));
        }
    }

    // 主灯带：只有模糊的光（stdDeviation 7）和一根亮芯。
    if let Some(path) = strip(1.0, 1.0) {
        let paint = strip_paint(s.accent, 1.0, 1.0);
        c.glow(7.0, 0.9, |l| l.stroke_path(&path, 14.0, true, paint));
        c.stroke_path(&path, 3.0, true, alpha(rgb(0xfff6e8), 0.85));
    }

    // 时间：text-shadow 0 0 30px 强调色（约 1/3 透明度）。
    let clock = d.clock();
    let big = TextStyle::num(118.0, 300).center().spacing_em(-0.01);
    let halo = alpha(s.accent, f32::from(0x55u8) / 255.0);
    c.glow(15.0, 1.0, |l| {
        l.text(&clock, 240.0, 243.7, &big, halo);
    });
    c.text(&clock, 240.0, 243.7, &big, rgb(FG));

    // 下面一行：“水温 27.0°   泵 2382”，标签雅黑 18px，数字 Bahnschrift 26px，间距 26。
    let label = TextStyle::ui(18.0);
    let num = TextStyle::num(26.0, 500);
    let (coolant, pump) = (d.coolant(), d.pump());
    let groups = [[("水温 ", false), (coolant.as_str(), true), ("°", false)], [("泵 ", false), (pump.as_str(), true), ("", false)]];
    let width = |c: &Canvas, g: &[(&str, bool); 3]| g.iter().map(|&(t, n)| c.measure(t, if n { &num } else { &label })).sum::<f32>();
    let total = width(c, &groups[0]) + 26.0 + width(c, &groups[1]);
    let mut x = 240.0 - total / 2.0;
    for g in &groups {
        for &(t, n) in g {
            x += if n { c.text(t, x, 301.2, &num, rgb(FG)) } else { c.text(t, x, 301.2, &label, rgb(LABEL)) };
        }
        x += 26.0;
    }
}
