//! 03 · 色块海报：上面一整块强调色铺满，压一个超大的粗体整数，小数和单位叠在右边；下面四格深浅交替的读数。

use crate::theme::{rgb, Canvas, Rgba, Style, TextStyle, ThemeData};

struct Palette {
    t1: Rgba,
    t2: Rgba,
    fg: Rgba,
    muted: Rgba,
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette { t1: rgb(0x1d1d1d), t2: rgb(0x262626), fg: rgb(0xf4f1ea), muted: rgb(0x9a9a9a) }
    } else {
        Palette { t1: rgb(0xf3f1ec), t2: rgb(0xe8e5de), fg: rgb(0x16140f), muted: rgb(0x7a766c) }
    }
}

/// 色块上的文字：两种模式都是近黑。
const INK: Rgba = rgb(0x151006);
/// 色块高度（下面 132px 是读数格）。
const SPLIT: f32 = 348.0;

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.t1);
    c.fill_rect(0.0, 0.0, 480.0, SPLIT, s.accent);

    // 色块顶部：标题和时间。
    c.text("水温", 30.0, 44.0, &TextStyle::ui(18.0).weight(600), INK);
    c.text( &d.clock(), 450.0, 42.0, &TextStyle::num(18.0, 600).right(), INK);

    // 大数字拆成整数和小数：“27” 特大，“.0” 和 “°C” 上下叠在右边。
    let value = d.coolant();
    let (int, dec) = match value.find('.') {
        Some(i) => value.split_at(i),
        None => (value.as_str(), ""),
    };
    let big = TextStyle::num(200.0, 700).spacing_em(-0.05);
    let w = c.text( int, 30.0, 307.0, &big, INK);
    // CSS 把最后一个字后的负字距也算进宽度，再加 6px 间距。
    let x = 30.0 + w + big.spacing + 6.0;
    c.text( dec, x, 217.0, &TextStyle::num(64.0, 700), INK);
    c.text( "°C", x, 257.0, &TextStyle::num(30.0, 600), INK);

    // 下面四格，第 2、4 格底色深一档。
    let cells = [("泵", d.pump()), ("风扇", format!("{}%", d.fan())), ("CPU", format!("{}{}", d.cpu(), d.cpu_unit())), ("GPU", format!("{}°C", d.gpu()))];
    let label = TextStyle::num(14.0, 400);
    let num = TextStyle::num(32.0, 600);
    for (i, (name, v)) in cells.iter().enumerate() {
        let x0 = 120.0 * i as f32;
        if i % 2 == 1 {
            c.fill_rect(x0, SPLIT, 120.0, 480.0 - SPLIT, p.t2);
        }
        let x = x0 + if i == 0 { 22.0 } else { 20.0 };
        c.text(name, x, 382.0, &label, p.muted);
        c.text( v, x, 416.0, &num, p.fg);
    }
}
