//! 05 · 极简大字：一个占满半屏的细体整数，小数和单位叠在右边，底下一根细进度线加一行读数。

use crate::theme::{rgb, Canvas, Rgba, Style, TextStyle, ThemeData};

struct Palette {
    bg: Rgba,
    fg: Rgba,
    muted: Rgba,
    track: Rgba,
    accent_text: Rgba,
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette { bg: rgb(0x050505), fg: rgb(0xecebe6), muted: rgb(0x6f767e), track: rgb(0x1e2125), accent_text: s.accent }
    } else {
        Palette { bg: rgb(0xf6f4ef), fg: rgb(0x141310), muted: rgb(0x8a857b), track: rgb(0xe2ded5), accent_text: darker(s.accent) }
    }
}

/// 亮底上强调色文字对比度不够，压暗一些（默认琥珀 #f2a33a → 设计稿的 #b8650e）。
fn darker(c: Rgba) -> Rgba {
    [(f32::from(c[0]) * 0.76) as u8, (f32::from(c[1]) * 0.62) as u8, (f32::from(c[2]) * 0.25) as u8, 255]
}

/// 左右内边距 44，内容区 44–436。
const L: f32 = 44.0;
const R: f32 = 436.0;

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.bg);

    // 顶行：标题和时间。
    c.text("水温", L, 60.0, &TextStyle::ui(20.0), p.accent_text);
    c.text( &d.clock(), R, 60.0, &TextStyle::num(20.0, 400).right(), p.muted);

    // 大数字：整数特大，“.0” 和 “°C” 叠在右边。
    let value = d.coolant();
    let (int, dec) = match value.find('.') {
        Some(i) => value.split_at(i),
        None => (value.as_str(), ""),
    };
    let big = TextStyle::num(236.0, 300).spacing_em(-0.04);
    let w = c.text( int, 36.0, 300.5, &big, p.fg);
    // CSS 把最后一个字后的负字距也算进宽度。
    let x = 36.0 + w + big.spacing;
    c.text( dec, x, 195.5, &TextStyle::num(72.0, 300), p.muted);
    c.text( "°C", x, 243.5, &TextStyle::num(34.0, 400), p.accent_text);

    // 进度线：4px 底槽 + 强调色填充 + 一根竖的游标。
    let (by, bw) = (398.0, R - L);
    c.fill_round_rect(L, by, bw, 4.0, 2.0, p.track);
    if d.coolant_c.is_some() {
        let fx = bw * d.coolant_frac();
        if fx > 0.0 {
            c.fill_round_rect(L, by, fx.max(4.0), 4.0, 2.0, s.accent);
        }
        c.fill_round_rect(L + fx - 2.0, by - 5.0, 4.0, 14.0, 2.0, p.fg);
    }

    // 底行三组“标签 数字”，两端对齐、中间那组居中分配。
    let items = [("泵 ", d.pump()), ("风扇 ", format!("{}%", d.fan())), ("CPU ", format!("{}{}", d.cpu(), d.cpu_unit()))];
    let st = TextStyle::num(18.0, 400);
    let widths: Vec<(f32, f32)> = items.iter().map(|(n, v)| (c.measure(n, &st), c.measure(v, &st))).collect();
    let total: f32 = widths.iter().map(|(a, b)| a + b).sum();
    let gap = (bw - total) / 2.0;
    let mut x = L;
    for ((name, v), (nw, vw)) in items.iter().zip(&widths) {
        c.text( name, x, 434.0, &st, p.muted);
        c.text( v, x + nw, 434.0, &st, p.fg);
        x += nw + vw + gap;
    }
}
