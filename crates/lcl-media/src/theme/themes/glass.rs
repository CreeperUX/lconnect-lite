//! 11 · 极光玻璃：三团大半径模糊的极光色块打底，上面一张水温大卡（带走势线）和四张读数小卡，都是毛玻璃。

use crate::theme::{alpha, rgb, rgba, Canvas, Paint, Rgba, Style, TextStyle, ThemeData};
use tiny_skia::{Path, PathBuilder, Rect};

/// 外边距 18、间距 12 的 2 列 × 3 行网格（第一行占满两列）。
const BIG: (f32, f32, f32, f32) = (18.0, 18.0, 444.0, 228.0);
const SMALL: [(f32, f32); 4] = [(18.0, 258.0), (246.0, 258.0), (18.0, 366.0), (246.0, 366.0)];
const SMALL_W: f32 = 216.0;
const SMALL_H: f32 = 96.0;
const BIG_R: f32 = 28.0;
const SMALL_R: f32 = 22.0;

struct Palette {
    bg: Rgba,
    fg: Rgba,
    muted: Rgba,
    blue: Rgba,
    violet: Rgba,
    /// 三团色块（强调色、蓝、紫）的不透明度。
    o: [f32; 3],
    glass: (Rgba, Rgba),
    border: Rgba,
    hi: Rgba,
    /// 卡片投影：（下移，模糊 sigma，颜色）。
    drop: (f32, f32, Rgba),
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette {
            bg: rgb(0x06070a),
            fg: rgb(0xf4f3ef),
            muted: rgba(0xffffff, 0.68),
            blue: rgb(0x2f7dff),
            violet: rgb(0xa259ff),
            o: [0.75, 0.7, 0.55],
            glass: (rgba(0xffffff, 0.16), rgba(0xffffff, 0.05)),
            border: rgba(0xffffff, 0.16),
            hi: rgba(0xffffff, 0.25),
            drop: (20.0, 20.0, rgba(0x000000, 0.35)),
        }
    } else {
        Palette {
            bg: rgb(0xeef0f5),
            fg: rgb(0x16181d),
            muted: rgba(0x16181d, 0.6),
            blue: rgb(0x7cc4ff),
            violet: rgb(0xc9a7ff),
            o: [0.55, 0.6, 0.55],
            glass: (rgba(0xffffff, 0.72), rgba(0xffffff, 0.40)),
            border: rgba(0xffffff, 0.9),
            hi: rgba(0xffffff, 0.95),
            drop: (12.0, 15.0, rgba(0x283250, 0.12)),
        }
    }
}

/// 所有卡片：（x, y, w, h, 圆角）。
fn cards() -> Vec<(f32, f32, f32, f32, f32)> {
    let mut v = vec![(BIG.0, BIG.1, BIG.2, BIG.3, BIG_R)];
    v.extend(SMALL.iter().map(|&(x, y)| (x, y, SMALL_W, SMALL_H, SMALL_R)));
    v
}

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.bg);

    // 极光色块：CSS 里各自 blur(70–80px)，这里合成一次模糊。
    let blobs = |l: &mut Canvas, k: f32| {
        oval(l, 90.0, 80.0, 150.0, 150.0, alpha(s.accent, p.o[0] * k));
        oval(l, 400.0, 390.0, 160.0, 150.0, alpha(p.blue, p.o[1] * k));
        oval(l, 350.0, 210.0, 100.0, 90.0, alpha(p.violet, p.o[2] * k));
    };
    c.glow(75.0, 1.0, |l| blobs(l, 1.0));

    let cards = cards();
    let all = |inset: f32, dy: f32, rev: bool, pb: &mut PathBuilder| {
        for &(x, y, w, h, r) in &cards {
            add_rr(pb, x + inset, y + inset + dy, w - 2.0 * inset, h - 2.0 * inset, r - inset, rev);
        }
    };

    // 卡片投影：box-shadow 只画在卡片外面，所以裁掉卡片本身的区域。
    let mut outside = PathBuilder::new();
    if let Some(r) = Rect::from_xywh(0.0, 0.0, 480.0, 480.0) {
        outside.push_rect(r);
    }
    all(0.0, 0.0, true, &mut outside);
    if let Some(outside) = outside.finish() {
        let (dy, sigma, col) = p.drop;
        c.clipped(&outside, |l| {
            l.glow(sigma, 1.0, |g| {
                for &(x, y, w, h, r) in &cards {
                    g.fill_round_rect(x, y + dy, w, h, r, col);
                }
            });
        });
    }

    // 毛玻璃：背景本来就是大半径模糊，backdrop blur 几乎看不出来；
    // saturate(150%) 用在卡片里再叠一层淡淡的极光色块来近似（颜色更浓）。
    if let Some(inside) = build(|pb| all(0.0, 0.0, false, pb)) {
        c.clipped(&inside, |l| l.glow(75.0, 0.35, |g| blobs(g, 1.0)));
    }
    for &(x, y, w, h, r) in &cards {
        let tint = css_linear(x, y, w, h, 160.0, &[(0.0, p.glass.0), (1.0, p.glass.1)]);
        c.fill_round_rect(x, y, w, h, r, tint);
    }
    // inset 0 1px 0：内边框顶上一道 1px 高光（沿着圆角变细）。
    if let (Some(pad), Some(hl)) = (
        build(|pb| all(1.0, 0.0, false, pb)),
        build(|pb| {
            all(1.0, 0.0, false, pb);
            all(1.0, 1.0, true, pb);
        }),
    ) {
        let hi = p.hi;
        c.clipped(&pad, |l| l.fill_path(&hl, hi));
    }
    for &(x, y, w, h, r) in &cards {
        c.stroke_round_rect(x + 0.5, y + 0.5, w - 1.0, h - 1.0, r - 0.5, 1.0, p.border);
    }

    // 大卡：标题行 + 大数字 + 走势线。内容区 = 卡片 - 1px 边框 - 22/24 内边距。
    let (left, right) = (BIG.0 + 25.0, BIG.0 + BIG.2 - 25.0);
    c.text("水温", left, 57.9, &TextStyle::ui(16.0), p.muted);
    c.text(&d.clock(), right, 55.3, &TextStyle::num(16.0, 400).right(), p.muted);
    let big = TextStyle::num(104.0, 300).spacing_em(-0.03);
    let w = c.text(&d.coolant(), left, 206.8, &big, p.fg);
    // °C 继承了大数字的字距（-3.12px）；最后一个字后面的字距也算。
    let unit = TextStyle { spacing: big.spacing, ..TextStyle::num(32.0, 300) };
    c.text("°C", left + w + big.spacing, 206.8, &unit, p.muted);
    sparkline(c, &d.coolant_history, right - 120.0, 167.0, alpha(p.fg, 0.85));

    // 小卡：名字在上，读数在下。
    let items = [
        ("泵速", d.pump(), "rpm"),
        ("风扇", d.fan(), "%"),
        ("CPU", d.cpu(), d.cpu_unit()),
        ("GPU", d.gpu(), "°C"),
    ];
    for (&(x, y), (name, value, unit)) in SMALL.iter().zip(items.iter()) {
        let lx = x + 19.0;
        c.text(name, lx, y + 29.8, &TextStyle::ui(14.0), p.muted);
        let w = c.text(value, lx, y + 74.0, &TextStyle::num(34.0, 400), p.fg);
        c.text(&format!(" {unit}"), lx + w, y + 74.0, &TextStyle::num(16.0, 400), p.muted);
    }
}

/// 走势线：120×56 的框里，最近十分钟水温从左到右；纵向至少按 0.5 °C 的跨度缩放，避免小抖动被放大。
fn sparkline(c: &mut Canvas, hist: &[f32], x: f32, y: f32, col: Rgba) {
    if hist.len() < 2 {
        return;
    }
    let lo = hist.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = hist.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mid = (lo + hi) / 2.0;
    let span = (hi - lo).max(0.5);
    // 设计稿里折线落在 y 26–42 之间。
    let n = hist.len() as f32 - 1.0;
    let pts: Vec<(f32, f32)> = hist
        .iter()
        .enumerate()
        .map(|(i, &v)| (x + 120.0 * i as f32 / n, y + 34.0 - (v - mid) / span * 16.0))
        .collect();
    c.stroke_polyline(&pts, 2.5, false, col);
}

fn oval(c: &mut Canvas, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgba) {
    if let Some(p) = Rect::from_xywh(cx - rx, cy - ry, rx * 2.0, ry * 2.0).and_then(PathBuilder::from_oval) {
        c.fill_path(&p, col);
    }
}

/// CSS 的 linear-gradient(<deg>)：0° 向上、顺时针，渐变线长度按 CSS 规则覆盖整个矩形。
fn css_linear(x: f32, y: f32, w: f32, h: f32, deg: f32, stops: &[(f32, Rgba)]) -> Paint {
    let a = deg.to_radians();
    let (dx, dy) = (a.sin(), -a.cos());
    let half = (w * dx.abs() + h * dy.abs()) / 2.0;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    Paint::linear((cx - dx * half, cy - dy * half), (cx + dx * half, cy + dy * half), stops)
}

fn build(f: impl FnOnce(&mut PathBuilder)) -> Option<Path> {
    let mut pb = PathBuilder::new();
    f(&mut pb);
    pb.finish()
}

/// 往路径里加一个圆角矩形；`rev` 为逆时针（和顺时针的叠在一起时按非零规则挖空）。
fn add_rr(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: f32, rev: bool) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = 0.552_284_8 * r;
    let (x1, y1) = (x + w, y + h);
    pb.move_to(x + r, y);
    if rev {
        pb.cubic_to(x + r - k, y, x, y + r - k, x, y + r);
        pb.line_to(x, y1 - r);
        pb.cubic_to(x, y1 - r + k, x + r - k, y1, x + r, y1);
        pb.line_to(x1 - r, y1);
        pb.cubic_to(x1 - r + k, y1, x1, y1 - r + k, x1, y1 - r);
        pb.line_to(x1, y + r);
        pb.cubic_to(x1, y + r - k, x1 - r + k, y, x1 - r, y);
    } else {
        pb.line_to(x1 - r, y);
        pb.cubic_to(x1 - r + k, y, x1, y + r - k, x1, y + r);
        pb.line_to(x1, y1 - r);
        pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
        pb.line_to(x + r, y1);
        pb.cubic_to(x + r - k, y1, x, y1 - r + k, x, y1 - r);
        pb.line_to(x, y + r);
        pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    }
    pb.close();
}
