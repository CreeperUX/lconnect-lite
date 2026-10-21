//! 02 · 单线：一条 1.5px 的 270° 细弧当量程，走过的部分用前景色、端点一颗强调色圆点；底部四个线框图标读数。

use crate::theme::{polar, rgb, Canvas, Rgba, Style, TextStyle, ThemeData};
use tiny_skia::PathBuilder;

const CX: f32 = 240.0;
const CY: f32 = 230.0;
const R: f32 = 158.0;
/// 弧从左下 135° 顺时针扫 270°。
const START: f32 = 135.0;
const SWEEP: f32 = 270.0;

struct Palette {
    bg: Rgba,
    fg: Rgba,
    muted: Rgba,
    track: Rgba,
    divider: Rgba,
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette { bg: rgb(0x000000), fg: rgb(0xf2f2f2), muted: rgb(0x8a8a8a), track: rgb(0x3a3a3a), divider: rgb(0x2a2a2a) }
    } else {
        Palette { bg: rgb(0xfafaf8), fg: rgb(0x141414), muted: rgb(0x7a7a7a), track: rgb(0xd9d9d6), divider: rgb(0xe2e2df) }
    }
}

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.bg);

    // 量程弧：整条轨道，再叠上走过的部分和端点圆点。
    c.stroke_arc(CX, CY, R, START, SWEEP, 1.5, false, p.track);
    if d.coolant_c.is_some() {
        let sweep = SWEEP * d.coolant_frac();
        c.stroke_arc(CX, CY, R, START, sweep, 1.5, false, p.fg);
        let (x, y) = polar(CX, CY, R, START + sweep);
        c.fill_circle(x, y, 6.0, s.accent);
    }

    // 文字。时间带 0.2em 字距，CSS 居中时把最后一个字后面的字距也算进去了，往左挪半个字距。
    let time = TextStyle::num(15.0, 400).center().spacing_em(0.2);
    c.text( &d.clock(), CX - time.spacing / 2.0, 47.0, &time, p.muted);
    c.text( "水温 °C", CX, 165.0, &TextStyle::num(15.0, 400).center(), p.muted);
    c.text( &d.coolant(), CX, 259.0, &TextStyle::num(104.0, 300).center(), p.fg);
    c.text( "20 — 50", CX, 308.0, &TextStyle::num(14.0, 400).center(), p.muted);

    // 底部四格：左右各留 24，每格 108 宽，格内图标 + 8px + 数字居中。
    let values = [d.pump(), format!("{}%", d.fan()), format!("{}{}", d.cpu(), d.cpu_unit()), format!("{}°C", d.gpu())];
    let num = TextStyle::num(18.0, 400);
    for (i, v) in values.iter().enumerate() {
        let x0 = 24.0 + 108.0 * i as f32;
        if i > 0 {
            c.fill_rect(x0, 424.0, 1.0, 22.0, p.divider);
        }
        let tw = c.measure(v, &num);
        let gx = x0 + 54.0 - (18.0 + 8.0 + tw) / 2.0;
        icon(c, i, gx, 426.0, p.fg);
        c.text( v, gx + 26.0, 440.0, &num, p.fg);
    }
}

/// 18×18 的线框图标（坐标照抄设计稿里的 SVG），左上角放在 (x, y)。
fn icon(c: &mut Canvas, kind: usize, x: f32, y: f32, col: Rgba) {
    let mut pb = PathBuilder::new();
    let k = 0.552_284_8;
    match kind {
        // 泵：水滴。M9 2 c0 4 -5 6 -5 10 a5 5 0 0 0 10 0 c0 -4 -5 -6 -5 -10 z
        0 => {
            pb.move_to(x + 9.0, y + 2.0);
            pb.cubic_to(x + 9.0, y + 6.0, x + 4.0, y + 8.0, x + 4.0, y + 12.0);
            let r = 5.0 * k;
            pb.cubic_to(x + 4.0, y + 12.0 + r, x + 9.0 - r, y + 17.0, x + 9.0, y + 17.0);
            pb.cubic_to(x + 9.0 + r, y + 17.0, x + 14.0, y + 12.0 + r, x + 14.0, y + 12.0);
            pb.cubic_to(x + 14.0, y + 8.0, x + 9.0, y + 6.0, x + 9.0, y + 2.0);
            pb.close();
        }
        // 风扇：圆 + 十字。
        1 => {
            pb.push_circle(x + 9.0, y + 9.0, 7.0);
            pb.move_to(x + 9.0, y + 2.0);
            pb.line_to(x + 9.0, y + 16.0);
            pb.move_to(x + 2.0, y + 9.0);
            pb.line_to(x + 16.0, y + 9.0);
        }
        // CPU：圆角方块 + 上下各两根针脚。
        2 => {
            push_round_rect(&mut pb, x + 3.0, y + 3.0, 12.0, 12.0, 2.0);
            for px in [7.0, 11.0] {
                pb.move_to(x + px, y + 1.0);
                pb.line_to(x + px, y + 3.0);
                pb.move_to(x + px, y + 15.0);
                pb.line_to(x + px, y + 17.0);
            }
        }
        // GPU：显卡外框 + 风扇圆。
        _ => {
            push_round_rect(&mut pb, x + 1.0, y + 4.0, 16.0, 10.0, 2.0);
            pb.push_circle(x + 9.0, y + 9.0, 2.5);
        }
    }
    if let Some(path) = pb.finish() {
        c.stroke_path(&path, 1.5, false, col);
    }
}

/// 往路径里追加一个圆角矩形子路径。
fn push_round_rect(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let k = 0.552_284_8 * r;
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
}
