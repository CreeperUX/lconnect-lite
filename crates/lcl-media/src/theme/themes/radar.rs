//! 15 · 雷达：同心圆、外圈刻度、扫描线后面拖着渐隐的扇形余晖；四个读数是按比例离中心远近的光点，中间是水温。

use crate::theme::{alpha, polar, rgb, Canvas, Paint, Rgba, Style, TextStyle, ThemeData};

const CX: f32 = 240.0;
const CY: f32 = 240.0;
const R: f32 = 210.0;
/// 光点和中间数字的颜色（偏白的薄荷绿）。
const WHITE: u32 = 0xeafff6;
/// 扫描线的角度（画布角度：0° 向右、顺时针）；余晖从它往回拖 58°。
const SWEEP_AT: f32 = -40.0;
const TRAIL: f32 = 58.0;
/// 余晖最亮处的不透明度（设计稿是强调色 + 0x55）。
const TRAIL_ALPHA: f32 = 0x55 as f32 / 255.0;

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let a = s.accent;
    c.fill_all(Paint::radial((CX, CY), 339.4, &[(0.0, rgb(0x06231d)), (0.55, rgb(0x021210)), (1.0, rgb(0x010605))]));

    sweep_trail(c, a);

    // 同心圆和十字线。
    c.stroke_circle(CX, CY, R, 1.0, alpha(a, 0.45));
    for r in [158.0, 106.0, 54.0] {
        c.stroke_circle(CX, CY, r, 1.0, alpha(a, 0.22));
    }
    c.stroke_line(CX - R, CY, CX + R, CY, 1.0, false, alpha(a, 0.22));
    c.stroke_line(CX, CY - R, CX, CY + R, 1.0, false, alpha(a, 0.22));

    // 外圈刻度：每 5° 一根，每 30° 一根长的。
    for i in 0..72 {
        let deg = i as f32 * 5.0;
        let len = if i % 6 == 0 { 14.0 } else { 7.0 };
        let (p0, p1) = (polar(CX, CY, R, deg), polar(CX, CY, R - len, deg));
        c.stroke_line(p0.0, p0.1, p1.0, p1.1, 1.0, false, alpha(a, 0.45));
    }

    // 扫描线。
    let tip = polar(CX, CY, R, SWEEP_AT);
    c.glow(3.0, 1.0, |l| l.stroke_line(CX, CY, tip.0, tip.1, 2.0, false, a));
    c.stroke_line(CX, CY, tip.0, tip.1, 2.0, false, a);

    // 光点：方向固定在四个斜角，离中心的距离按比例。设计稿是 60 + 比例 × 140，
    // 但比例很小（CPU 空闲、没有读数）时光点和标签会压到中间的数字和“COOLANT °C”上，所以距离最小取 88。
    // （名字, 读数, 比例, 设计稿角度, 标签偏移）
    let blips = [
        ("PUMP", d.pump(), d.pump_frac(), 40.0, (12.0, -14.0)),
        ("FAN", unit(d.fan(), "%"), d.fan_frac(), 130.0, (12.0, -6.0)),
        ("CPU", unit(d.cpu(), d.cpu_unit()), d.cpu_frac(), 220.0, (-50.0, -6.0)),
        ("GPU", unit(d.gpu(), "°C"), d.gpu_frac(), 310.0, (-50.0, -26.0)),
    ];
    let pos: Vec<(f32, f32)> = blips.iter().map(|b| polar(CX, CY, (60.0 + b.2 * 140.0).max(88.0), b.3 - 90.0)).collect();
    c.glow(4.0, 0.5, |l| {
        for &(x, y) in &pos {
            l.fill_circle(x, y, 9.0, a);
        }
    });
    let name = TextStyle::num(11.0, 400).spacing_em(0.14);
    let value = TextStyle::num(18.0, 400);
    for (b, &(x, y)) in blips.iter().zip(&pos) {
        c.fill_circle(x, y, 4.0, rgb(WHITE));
        // 标签左上角 = 光点 + 偏移；两行 line-height 1.15。
        let (lx, ly) = (x + b.4 .0, y + b.4 .1);
        c.text(b.0, lx, ly + 9.6, &name, alpha(a, 0.75));
        c.text(&b.1, lx, ly + 28.3, &value, rgb(WHITE));
    }

    // 中间水温。
    let coolant = d.coolant();
    let big = TextStyle::num(76.0, 500).center();
    c.glow(9.0, 1.0, |l| {
        l.text(&coolant, CX, 256.3, &big, a);
    });
    c.text(&coolant, CX, 256.3, &big, rgb(WHITE));
    // text-indent 和字距相等，左右对称，直接居中。
    c.text("COOLANT °C", CX, 286.5, &TextStyle::num(14.0, 400).center().spacing_em(0.3), a);

    // 右下角时间（最后一个字后面的字距也占位）。
    let clock = TextStyle::num(14.0, 400).right().spacing_em(0.15);
    c.text(&d.clock(), 460.0 - clock.spacing, 459.7, &clock, alpha(a, 0.8));
}

fn unit(v: String, u: &str) -> String {
    if v == "—" {
        v
    } else {
        v + u
    }
}

/// 余晖：conic-gradient 从扫描线往回 58° 由亮到透明，扫描线之后 2° 迅速消失。
/// 用一组共用终边、起点逐渐后移的扇形叠出来，每层的透明度算好使叠加后正好是线性渐变，没有拼缝。
fn sweep_trail(c: &mut Canvas, a: Rgba) {
    const N: usize = 32;
    let end = SWEEP_AT;
    let mut prev = 0.0f32;
    for k in 1..=N {
        // 第 k 层覆盖 [start_k, end]，叠加后 [start_k, start_{k+1}] 区间的不透明度为 TRAIL_ALPHA × k / N。
        let start = end - TRAIL + TRAIL * (k - 1) as f32 / N as f32;
        let target = TRAIL_ALPHA * k as f32 / N as f32;
        let col = alpha(a, 1.0 - (1.0 - target) / (1.0 - prev));
        // 颜色的透明度会取整到 1/255，按取整后的值累计，误差不往后传。
        prev = 1.0 - (1.0 - prev) * (1.0 - f32::from(col[3]) / 255.0);
        wedge(c, start, end, col);
    }
    // 扫描线前面 2°：从最亮迅速降到 0。
    wedge(c, end, end + 1.0, alpha(a, TRAIL_ALPHA * 0.75));
    wedge(c, end + 1.0, end + 2.0, alpha(a, TRAIL_ALPHA * 0.25));
}

fn wedge(c: &mut Canvas, from: f32, to: f32, col: Rgba) {
    let steps = ((to - from).abs().ceil() as usize).max(1);
    let mut pts = vec![(CX, CY)];
    pts.extend((0..=steps).map(|i| polar(CX, CY, R, from + (to - from) * i as f32 / steps as f32)));
    c.fill_polygon(&pts, col);
}
