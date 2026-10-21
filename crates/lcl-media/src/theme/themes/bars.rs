//! 09 · 柱状条：顶部水温大读数和时间，下面五根带横纹的竖槽（水温 / 泵 / 风扇 / CPU / GPU），液柱带渐变和柔光。

use crate::theme::{rgb, rgba, round_rect, Canvas, Paint, Rgba, Style, TextStyle, ThemeData};
use tiny_skia::PathBuilder;

/// 左右留白。
const PAD_X: f32 = 28.0;
/// 标题行的基线（54px Bahnschrift，顶边 28）。
const HEAD_BASE: f32 = 28.0 + 0.794 * 54.0;
/// 柱区顶边：标题行高 54 + 间距 16。
const GRID_TOP: f32 = 28.0 + 54.0 + 16.0;
/// 柱区底边（底部留白 24）。
const GRID_BOTTOM: f32 = 480.0 - 24.0;
const GAP: f32 = 12.0;
const RADIUS: f32 = 14.0;

struct Palette {
    bg: Paint,
    fg: Rgba,
    muted: Rgba,
    well: Rgba,
    /// 槽内阴影：颜色、模糊（sigma）。
    inset: (Rgba, f32),
    /// 横纹颜色。
    gap: Rgba,
    /// 液柱柔光的透明度。
    glow: f32,
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette {
            bg: Paint::linear((0.0, 0.0), (0.0, 480.0), &[(0.0, rgb(0x121519)), (1.0, rgb(0x08090b))]),
            fg: rgb(0xecebe6),
            muted: rgb(0x8c939b),
            well: rgb(0x171a1e),
            inset: (rgba(0x000000, 0.6), 3.0),
            gap: rgba(0x08090b, 0.55),
            glow: 0x55 as f32 / 255.0,
        }
    } else {
        Palette {
            bg: Paint::linear((0.0, 0.0), (0.0, 480.0), &[(0.0, rgb(0xf7f8fa)), (1.0, rgb(0xe9ecf0))]),
            fg: rgb(0x14171b),
            muted: rgb(0x6b727a),
            well: rgb(0xdfe3e8),
            inset: (rgba(0x141e28, 0.12), 2.5),
            gap: rgba(0xe9ecf0, 0.9),
            glow: 0x33 as f32 / 255.0,
        }
    }
}

/// 一根柱子。
struct Col {
    name: &'static str,
    value: String,
    frac: f32,
    top: Rgba,
    bottom: Rgba,
    x: f32,
    w: f32,
    /// 槽的上下边。
    y0: f32,
    y1: f32,
}

/// 读数加单位；缺失时只显示“—”。
fn with_unit(v: String, u: &str) -> String {
    if v == "—" {
        v
    } else {
        v + u
    }
}

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.bg.clone());

    // ---- 标题行：左边“27.0 °C 水温”，右边时间，基线对齐 ----
    let w = c.text(&d.coolant(), PAD_X, HEAD_BASE, &TextStyle::num(54.0, 400), p.fg);
    c.text("°C 水温", PAD_X + w + 8.0, HEAD_BASE, &TextStyle::num(20.0, 400), p.muted);
    c.text(&d.clock(), 480.0 - PAD_X, HEAD_BASE, &TextStyle::num(18.0, 400).right(), p.muted);

    // ---- 五列：第一列 1.4 份宽，其余各 1 份，间距 12 ----
    let unit = (480.0 - 2.0 * PAD_X - 4.0 * GAP) / 5.4;
    let specs: [(&str, String, Option<f32>, Rgba, Rgba); 5] = [
        ("水温", with_unit(d.coolant(), "°C"), d.coolant_c.map(|_| d.coolant_frac()), s.accent, rgb(0xc4561c)),
        ("泵", d.pump(), d.pump_rpm.map(|_| d.pump_frac()), rgb(0x7fc4f0), rgb(0x2e6e9c)),
        ("风扇", with_unit(d.fan(), "%"), d.fan_percent.map(|_| d.fan_frac()), rgb(0x6fd29c), rgb(0x2b7a50)),
        ("CPU", with_unit(d.cpu(), d.cpu_unit()), d.has_cpu().then(|| d.cpu_frac()), rgb(0xc9b9f2), rgb(0x6a54b4)),
        ("GPU", with_unit(d.gpu(), "°C"), d.gpu_temp_c.map(|_| d.gpu_frac()), rgb(0xf0a2c4), rgb(0xa8487a)),
    ];
    let mut x = PAD_X;
    let cols: Vec<Col> = specs
        .into_iter()
        .enumerate()
        .map(|(i, (name, value, frac, top, bottom))| {
            let w = if i == 0 { unit * 1.4 } else { unit };
            // 数值行高 24（Bahnschrift 20px），标签行高：中文回退雅黑约 18.5，纯英文 16.8；上下各隔 8。
            let name_h = if name.is_ascii() { 16.8 } else { 18.5 };
            let col = Col { name, value, frac: frac.unwrap_or(0.0), top, bottom, x, w, y0: GRID_TOP + 24.0 + 8.0, y1: GRID_BOTTOM - name_h - 8.0 };
            x += w + GAP;
            col
        })
        .collect();

    // 所有槽合成一条裁剪路径，阴影、柔光、液柱和横纹一次画完。
    let mut pb = PathBuilder::new();
    for col in &cols {
        if let Some(r) = round_rect(col.x, col.y0, col.w, col.y1 - col.y0, RADIUS) {
            pb.push_path(&r);
        }
    }
    let Some(clip) = pb.finish() else { return };
    let fill_rect = |col: &Col| {
        let h = (col.y1 - col.y0) * (col.frac * 100.0).round() / 100.0;
        (col.y1 - h, h)
    };
    c.fill_path(&clip, p.well);
    c.clipped(&clip, |l| {
        // 槽内阴影（inset 0 2px）：槽外一圈往下挪 2px，模糊后只留槽里的部分。
        let (shadow, sigma) = p.inset;
        l.glow(sigma, 1.0, |g| {
            for col in &cols {
                let k = 8.0;
                g.stroke_round_rect(col.x - k, col.y0 + 2.0 - k, col.w + 2.0 * k, col.y1 - col.y0 + 2.0 * k, RADIUS + k, 2.0 * k, shadow);
            }
        });
        // 液柱柔光（box-shadow 0 0 18px）。
        l.glow(9.0, p.glow, |g| {
            for col in &cols {
                let (y, h) = fill_rect(col);
                if h > 0.0 {
                    g.fill_rect(col.x, y, col.w, h, col.top);
                }
            }
        });
        for col in &cols {
            let (y, h) = fill_rect(col);
            if h > 0.0 {
                l.fill_rect(col.x, y, col.w, h, Paint::linear((0.0, y), (0.0, col.y1), &[(0.0, col.top), (1.0, col.bottom)]));
            }
            // 横纹：每 12px 一道 2px。
            let mut sy = col.y0 + 10.0;
            while sy < col.y1 {
                l.fill_rect(col.x, sy, col.w, 2.0_f32.min(col.y1 - sy), p.gap);
                sy += 12.0;
            }
        }
    });

    // ---- 文字：上面数值，下面标签 ----
    for col in &cols {
        let cx = col.x + col.w / 2.0;
        c.text(&col.value, cx, GRID_TOP + 0.894 * 20.0, &TextStyle::num(20.0, 400).center(), p.fg);
        c.text(col.name, cx, GRID_BOTTOM - 0.306 * 14.0, &TextStyle::num(14.0, 400).center(), p.muted);
    }
}
