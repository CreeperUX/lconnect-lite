//! 13 · 霓虹：合成波夜空、条纹落日、透视网格地面，大号霓虹水温（多层辉光），底部四个霓虹小框。

use crate::theme::{rgb, rgba, Canvas, Paint, Rgba, Style, TextStyle, ThemeData};
use tiny_skia::{PathBuilder, Rect};

const PINK: u32 = 0xff4fa3;
const VIOLET: u32 = 0xa53cff;
const CYAN: u32 = 0x2fd3ff;
const ICE: u32 = 0x7ef6ff;
/// 地平线。
const HORIZON: f32 = 296.0;
/// 底部四个小框：左右各 22、间距 8、底边距 26；高度按 8px 内边距 + 两行文字 + 1.5px 边框算。
const CHIP_Y: f32 = 395.4;
const CHIP_W: f32 = 103.0;
const CHIP_H: f32 = 58.6;
const CHIP_R: f32 = 12.0;

fn chip_x(i: usize) -> f32 {
    22.0 + i as f32 * (CHIP_W + 8.0)
}

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    c.fill_all(Paint::linear(
        (0.0, 0.0),
        (0.0, 480.0),
        &[(0.0, rgb(0x12051f)), (0.46, rgb(0x1d0830)), (0.62, rgb(0x07020c)), (1.0, rgb(0x050108))],
    ));

    sun(c);

    // 地面和透视网格（整组 0.85 透明度，交叉处不叠加）。
    c.fill_rect(0.0, HORIZON, 480.0, 480.0 - HORIZON, rgb(0x07020c));
    let grid = s.accent;
    c.layer(0.85, |l| {
        for i in -6..=6 {
            l.stroke_line(240.0, HORIZON, 240.0 + i as f32 * 110.0, 480.0, 1.3, false, grid);
        }
        for y in [302.0, 312.0, 326.0, 344.0, 368.0, 400.0, 442.0] {
            l.stroke_line(0.0, y, 480.0, y, 1.3, false, grid);
        }
    });
    c.stroke_line(0.0, HORIZON, 480.0, HORIZON, 2.0, false, rgb(PINK));

    // 文字内容。
    let value = format!("{}°", d.coolant());
    let big = TextStyle::num(120.0, 600).center().spacing_em(0.02);
    // CSS 居中时把最后一个字后面的字距也算进宽度，所以往左偏半个字距。
    let big_x = 240.0 - big.spacing / 2.0;
    let big_y = 169.3;
    let top = TextStyle::num(16.0, 400).spacing_em(0.3);
    let clock = d.clock();
    let top_text = |l: &mut Canvas, col: Rgba| {
        l.text("COOLANT", 30.0, 48.3, &top, col);
        l.text(&clock, 450.0 - top.spacing, 48.3, &TextStyle { align: crate::theme::Align::Right, ..top }, col);
    };
    let chips = [("PUMP", d.pump(), ""), ("FAN", d.fan(), "%"), ("CPU", d.cpu(), d.cpu_unit()), ("GPU", d.gpu(), "°C")];
    let chip_values: Vec<String> = chips.iter().map(|(_, v, u)| if v == "—" { v.clone() } else { format!("{v}{u}") }).collect();
    let val_style = TextStyle::num(22.0, 400).center();
    let chip_text = |l: &mut Canvas, col: Rgba| {
        for (i, v) in chip_values.iter().enumerate() {
            l.text(v, chip_x(i) + CHIP_W / 2.0, 437.8, &val_style, col);
        }
    };

    // 大数字的多层 text-shadow：80px 紫、42px 粉、18px 粉（和顶栏 18px 青光同一遍）、6px 白。
    c.glow(40.0, 1.0, |l| {
        l.text(&value, big_x, big_y, &big, rgb(VIOLET));
    });
    c.glow(21.0, 1.0, |l| {
        l.text(&value, big_x, big_y, &big, rgb(PINK));
    });
    c.glow(9.0, 1.0, |l| {
        l.text(&value, big_x, big_y, &big, rgb(PINK));
        top_text(l, rgb(CYAN));
    });

    // 小框：半透明暗底。
    for i in 0..4 {
        c.fill_round_rect(chip_x(i), CHIP_Y, CHIP_W, CHIP_H, CHIP_R, rgba(0x050108, 0.6));
    }

    // 剩下的小半径光合成一遍（省模糊次数）：
    // 地平线 6px 粉光、大数字 6px 白光、顶栏和小框读数 8px 青光，
    // 以及小框的外发光 + 内发光（box-shadow 0 0 10px 与 inset 0 0 10px）——用沿边框的一圈宽描边模糊后近似，框内外各一半。
    c.glow(4.0, 1.0, |l| {
        l.stroke_line(0.0, HORIZON, 480.0, HORIZON, 2.0, false, rgb(PINK));
        l.text(&value, big_x, big_y, &big, rgb(0xffffff));
        top_text(l, rgb(CYAN));
        chip_text(l, rgb(CYAN));
        for i in 0..4 {
            l.stroke_round_rect(chip_x(i), CHIP_Y, CHIP_W, CHIP_H, CHIP_R, 6.0, rgba(CYAN, 0.6));
        }
    });
    for i in 0..4 {
        c.stroke_round_rect(chip_x(i) + 0.75, CHIP_Y + 0.75, CHIP_W - 1.5, CHIP_H - 1.5, CHIP_R - 0.75, 1.5, rgb(ICE));
    }

    // 清晰的本体。
    top_text(c, rgb(ICE));
    c.text(&value, big_x, big_y, &big, rgb(0xfff5fb));
    let name = TextStyle::num(11.0, 400).center().spacing_em(0.15);
    for (i, (n, _, _)) in chips.iter().enumerate() {
        c.text(n, chip_x(i) + CHIP_W / 2.0 - name.spacing / 2.0, 414.7, &name, rgb(ICE));
    }
    chip_text(c, rgb(0xffffff));
}

/// 落日：上黄下紫的渐变圆，横向条纹遮罩（14px 实、4px 空）。
/// CSS 里 drop-shadow 先于 mask 生效，所以粉色光晕也被条纹切开、只留在 200×200 的方框里。
fn sun(c: &mut Canvas) {
    let (x, y, size) = (140.0, 168.0, 200.0);
    let mut pb = PathBuilder::new();
    let mut top = y;
    while top < y + size {
        if let Some(r) = Rect::from_xywh(x, top, size, 14.0f32.min(y + size - top)) {
            pb.push_rect(r);
        }
        top += 18.0;
    }
    let Some(stripes) = pb.finish() else { return };
    let (cx, cy, r) = (x + size / 2.0, y + size / 2.0, size / 2.0);
    c.layer(0.85, |l| {
        l.clipped(&stripes, |m| {
            m.glow(12.0, 0.6, |g| g.fill_circle(cx, cy, r, rgb(PINK)));
            m.fill_circle(cx, cy, r, Paint::linear((0.0, y), (0.0, y + size), &[(0.0, rgb(0xffd36b)), (0.6, rgb(PINK)), (1.0, rgb(VIOLET))]));
        });
    });
}
