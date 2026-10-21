//! 01 · 瑞士网格：粗黑体大数字 + 一块强调色方块，下面 2×2 细线网格放泵 / 风扇 / CPU / GPU。

use crate::theme::{rgb, Canvas, Rgba, Style, TextStyle, ThemeData};

struct Palette {
    bg: Rgba,
    fg: Rgba,
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette { bg: rgb(0x0f0f10), fg: rgb(0xf1efea) }
    } else {
        Palette { bg: rgb(0xf1efea), fg: rgb(0x111111) }
    }
}

/// 外边距 28，内容区 28–452。
const L: f32 = 28.0;
const R: f32 = 452.0;
/// 两列的分界。
const MID: f32 = 240.0;

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.bg);

    // 顶栏：标题和时间，下面一道 3px 粗线。
    let head = TextStyle::num(16.0, 600).spacing_em(0.04);
    c.text("COOLANT 水温", L, 44.0, &head, p.fg);
    c.text( &d.clock(), R - head.spacing, 44.0, &head.right(), p.fg);
    c.fill_rect(L, 59.0, R - L, 3.0, p.fg);

    // 大数字，右上角跟一块 40×40 的强调色方块（flex gap 8）。
    let big = TextStyle::num(168.0, 600).spacing_em(-0.04);
    let w = c.text( &d.coolant(), L, 198.0, &big, p.fg);
    // CSS 的字距在最后一个字后面也算宽度。
    c.fill_rect(L + w + big.spacing + 8.0, 86.0, 40.0, 40.0, s.accent);

    // 2×2 网格：每行顶上一道 1.5px 线，右列左边一道竖线。
    let rows = [
        (296.0, 381.0, [("泵 PUMP", d.pump()), ("风扇 FAN", format!("{}%", d.fan()))]),
        (381.0, 452.0, [("CPU", format!("{}{}", d.cpu(), d.cpu_unit())), ("GPU", format!("{}°C", d.gpu()))]),
    ];
    let label = TextStyle::num(13.0, 600);
    let value = TextStyle::num(40.0, 600);
    for (top, bottom, cells) in &rows {
        c.fill_rect(L, *top, R - L, 1.5, p.fg);
        c.fill_rect(MID, *top, 1.5, bottom - top, p.fg);
        for (i, (name, v)) in cells.iter().enumerate() {
            let x = if i == 0 { L } else { MID + 17.0 };
            c.text(name, x, top + 26.0, &label, p.fg);
            c.text( v, x, top + 64.0, &value, p.fg);
        }
    }
}
