//! 10 · 光环表盘：0.5 °C 一格的外圈刻度，蓝→琥珀→红的渐变进度弧带辉光，中间玻璃（暗）/ 白瓷（亮）圆盘。

use crate::theme::{alpha, polar, rgb, rgba, Canvas, Paint, Rgba, Style, TextStyle, ThemeData};

const CX: f32 = 240.0;
const CY: f32 = 250.0;
const R: f32 = 180.0;
/// 弧从左下 135° 顺时针扫 270°。
const START: f32 = 135.0;
const SWEEP: f32 = 270.0;

struct Palette {
    bg: Paint,
    fg: Rgba,
    muted: Rgba,
    faint: Rgba,
    cold: Rgba,
    track_out: Rgba,
    track_in: Rgba,
    disc: (Rgba, Rgba),
    rim: (Rgba, Rgba),
    disc_shadow: f32,
    knob_ring: Rgba,
    glow: f32,
    grain: f32,
    num: (Rgba, Rgba),
    accent_text: Rgba,
    ok: (Rgba, Rgba),
    bar: (Rgba, Rgba),
    bar_shadow: bool,
    divider: Rgba,
    lit: (Rgba, Rgba),
    dim: (Rgba, Rgba),
    cols: [Rgba; 4],
}

fn palette(s: &Style) -> Palette {
    if s.dark {
        Palette {
            bg: Paint::radial((240.0, 211.0), 340.0, &[(0.0, rgb(0x1c222a)), (0.48, rgb(0x0d1014)), (1.0, rgb(0x040506))]),
            fg: rgb(0xf1efe9),
            muted: rgb(0x7f8790),
            faint: rgb(0x5d646c),
            cold: rgb(0x38c6ff),
            track_out: rgb(0x06080a),
            track_in: rgb(0x151a1f),
            disc: (rgb(0x20262e), rgb(0x0b0e11)),
            rim: (rgba(0xffffff, 0.16), rgba(0xffffff, 0.05)),
            disc_shadow: 0.0,
            knob_ring: rgb(0xf6fbff),
            glow: 0.9,
            grain: 0.06,
            num: (rgb(0xffffff), rgb(0xc9cfd6)),
            accent_text: s.accent,
            ok: (rgba(0x5bc08a, 0.12), rgb(0x6fd29c)),
            bar: (rgba(0xffffff, 0.06), rgba(0xffffff, 0.02)),
            bar_shadow: false,
            divider: rgba(0xffffff, 0.06),
            lit: (rgb(0xe8f7ff), rgb(0x7fd8ff)),
            dim: (rgb(0x3b4148), rgb(0x22272c)),
            cols: [rgb(0x6fb3e0), rgb(0x5bc08a), rgb(0xb9a7e8), rgb(0xe58fb6)],
        }
    } else {
        Palette {
            bg: Paint::radial((240.0, 192.0), 340.0, &[(0.0, rgb(0xffffff)), (0.55, rgb(0xeef1f4)), (1.0, rgb(0xdde2e8))]),
            fg: rgb(0x16191d),
            muted: rgb(0x6c737b),
            faint: rgb(0x9aa1a8),
            cold: rgb(0x1aa3e8),
            track_out: rgb(0xd3d9df),
            track_in: rgb(0xe9edf1),
            disc: (rgb(0xffffff), rgb(0xeef1f4)),
            rim: (rgb(0xffffff), rgb(0xc9d0d7)),
            disc_shadow: 0.12,
            knob_ring: rgb(0x1aa3e8),
            glow: 0.45,
            grain: 0.03,
            num: (rgb(0x1a1d21), rgb(0x4a5058)),
            accent_text: darker(s.accent),
            ok: (rgba(0x2f9d66, 0.12), rgb(0x2a8a59)),
            bar: (rgb(0xffffff), rgb(0xffffff)),
            bar_shadow: true,
            divider: rgba(0x141e28, 0.08),
            lit: (rgb(0x1a8fd0), rgb(0x7cc8ef)),
            dim: (rgb(0xb9c1c9), rgb(0xd6dce2)),
            cols: [rgb(0x3a8cc4), rgb(0x2f9d66), rgb(0x8a72d6), rgb(0xd0628f)],
        }
    }
}

/// 亮底上强调色文字对比度不够，压暗一些。
fn darker(c: Rgba) -> Rgba {
    [(f32::from(c[0]) * 0.78) as u8, (f32::from(c[1]) * 0.66) as u8, (f32::from(c[2]) * 0.5) as u8, 255]
}

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let p = palette(s);
    c.fill_all(p.bg.clone());

    // 外圈刻度：20–50 °C 每 0.5 °C 一格，每 5 °C 一根长刻度，已经走过的点亮。
    let frac = d.coolant_frac();
    let lit_until = (frac * 60.0).round() as i32;
    for i in 0..=60 {
        // 正上方让给时间。
        if (28..=32).contains(&i) {
            continue;
        }
        let major = i % 10 == 0;
        let deg = START + i as f32 * 4.5;
        let (r0, r1) = (216.0, if major { 230.0 } else { 223.0 });
        let lit = d.coolant_c.is_some() && i <= lit_until;
        let col = match (lit, major) {
            (true, true) => p.lit.0,
            (true, false) => p.lit.1,
            (false, true) => p.dim.0,
            (false, false) => p.dim.1,
        };
        let (a, b) = (polar(CX, CY, r0, deg), polar(CX, CY, r1, deg));
        c.stroke_line(a.0, a.1, b.0, b.1, if major { 2.5 } else { 1.2 }, true, col);
    }

    // 轨道：外面一圈深色描边，里面一圈底色，做出凹槽感。
    c.stroke_arc(CX, CY, R, START, SWEEP, 26.0, true, p.track_out);
    c.stroke_arc(CX, CY, R, START, SWEEP, 20.0, true, p.track_in);

    let heat = Paint::linear((70.0, 380.0), (410.0, 380.0), &[(0.0, p.cold), (0.55, s.accent), (1.0, rgb(0xff4d5e))]);
    let sweep = SWEEP * frac;
    if sweep > 0.5 {
        let h = heat.clone();
        c.glow(9.0, p.glow, |l| l.stroke_arc(CX, CY, R, START, sweep, 22.0, true, h));
        c.stroke_arc(CX, CY, R, START, sweep, 16.0, true, heat);
        // 弧面上一道细高光。
        c.stroke_arc(CX + 1.5, CY - 1.5, R, START, sweep, 3.0, true, rgba(0xffffff, 0.35));
    }

    // 中间圆盘（亮色模式下带投影）。
    if p.disc_shadow > 0.0 {
        c.glow(12.0, p.disc_shadow, |l| l.fill_circle(CX, CY + 12.0, 146.0, rgb(0x000000)));
    }
    c.fill_circle(CX, CY, 146.0, Paint::radial((240.0, 191.6), 219.0, &[(0.0, p.disc.0), (1.0, p.disc.1)]));
    c.stroke_circle(CX, CY, 145.5, 1.5, Paint::linear((0.0, 104.0), (0.0, 396.0), &[(0.0, p.rim.0), (1.0, p.rim.1)]));

    // 进度端点的小圆钮。
    if sweep > 0.5 {
        let (kx, ky) = polar(CX, CY, R, START + sweep);
        let cold = p.cold;
        c.glow(4.0, 0.75, |l| l.fill_circle(kx, ky, 15.0, cold));
        c.fill_circle(kx, ky, 9.0, rgb(0xffffff));
        c.stroke_circle(kx, ky, 9.0, 2.0, p.knob_ring);
    }

    c.noise(p.grain, 7);

    // 文字。
    c.text(&d.clock(), CX, 36.0, &TextStyle::num(15.0, 400).center().spacing_em(0.18), p.faint);
    c.text("水温", CX, 157.0, &TextStyle::ui(15.0).center().spacing_em(0.4), p.muted);
    let value = d.coolant();
    let big = TextStyle::num(118.0, 300).spacing_em(-0.03);
    let unit = TextStyle::num(28.0, 400);
    let (w1, w2) = (c.measure(&value, &big), c.measure("°C", &unit));
    let x = CX - (w1 + 4.0 + w2) / 2.0;
    c.text(&value, x, 258.0, &big, Paint::linear((0.0, 175.0), (0.0, 260.0), &[(0.0, p.num.0), (1.0, p.num.1)]));
    c.text("°C", x + w1 + 4.0, 206.0, &unit, p.accent_text);

    // 趋势小标签。
    let (label, dir) = d.trend_label();
    let (bg, fg) = match dir {
        0 => p.ok,
        1 => (alpha(s.accent, 0.14), p.accent_text),
        _ => (rgba(0x3aa0e8, 0.14), if s.dark { rgb(0x7cc8ef) } else { rgb(0x1a7fbf) }),
    };
    let tw = c.measure(label, &TextStyle::ui(14.0));
    c.fill_round_rect(CX - tw / 2.0 - 12.0, 300.0, tw + 24.0, 26.0, 13.0, bg);
    c.text(label, CX, 318.0, &TextStyle::ui(14.0).center(), fg);

    // 底部读数条。
    let (bx, by, bw, bh) = (92.0, 394.0, 296.0, 56.0);
    if p.bar_shadow {
        c.glow(10.0, 0.12, |l| l.fill_round_rect(bx, by + 8.0, bw, bh, 18.0, rgb(0x14202c)));
    } else {
        c.glow(10.0, 0.5, |l| l.fill_round_rect(bx, by + 8.0, bw, bh, 18.0, rgb(0x000000)));
    }
    c.fill_round_rect(bx, by, bw, bh, 18.0, Paint::linear((0.0, by), (0.0, by + bh), &[(0.0, p.bar.0), (1.0, p.bar.1)]));
    c.stroke_line(bx + 18.0, by + 0.75, bx + bw - 18.0, by + 0.75, 1.0, false, if s.dark { rgba(0xffffff, 0.08) } else { rgba(0xffffff, 1.0) });
    let items = [("泵", d.pump()), ("风扇", format!("{}%", d.fan())), ("CPU", format!("{}{}", d.cpu(), d.cpu_unit())), ("GPU", format!("{}°C", d.gpu()))];
    let cw = bw / 4.0;
    for (i, (label, value)) in items.iter().enumerate() {
        let cx = bx + cw * (i as f32 + 0.5);
        if i > 0 {
            c.stroke_line(bx + cw * i as f32, by + 10.0, bx + cw * i as f32, by + bh - 10.0, 1.0, false, p.divider);
        }
        c.text(label, cx, by + 23.0, &TextStyle::ui(11.0).center(), p.cols[i]);
        c.text(value, cx, by + 45.0, &TextStyle::num(19.0, 500).center(), p.fg);
    }
}
