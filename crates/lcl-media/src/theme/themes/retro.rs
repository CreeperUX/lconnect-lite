//! 08 · 复古液晶：墨绿底、扫描线，带辉光的段码大读数（底下垫着暗淡的“88.8”残影），四行 20 格分段条。只有暗色。

use crate::theme::{alpha, rgb, rgba, Canvas, Paint, Rgba, Style, TextStyle, ThemeData};

/// 四周留白。
const PAD: f32 = 26.0;
/// 标题行（Cascadia 15px，中文回退到雅黑，行高约 19px）的基线。
const HEAD_BASE: f32 = 41.2;
/// 大读数区域的顶边：标题行高 + 18px 间距。
const BIG_TOP: f32 = 63.0;
/// 四行分段条的第一行顶边：大读数区 168px + 18px 间距。
const ROWS_TOP: f32 = BIG_TOP + 168.0 + 18.0;
/// 行高（Cascadia 15px）+ 行距 14px。
const ROW_STEP: f32 = 17.4 + 14.0;
const CELLS: usize = 20;

pub fn render(c: &mut Canvas, d: &ThemeData, s: &Style) {
    let acc = s.accent;
    c.fill_all(Paint::radial((240.0, 192.0), 375.0, &[(0.0, rgb(0x102217)), (0.75, rgb(0x07100a))]));

    // ---- 标题行和四行分段条（画在扫描线下面）----
    let head = TextStyle::mono(15.0).spacing_em(0.12);
    let clock = d.clock();
    // CSS 的字距在最后一个字后面也算，右对齐时可见字形离右边多出一个字距。
    let clock_x = 480.0 - PAD - head.spacing;

    let pump = d.pump();
    let fan = pct(&d.fan());
    let cpu = d.cpu();
    let cpu = if cpu == "—" { cpu } else { format!("{cpu}{}", d.cpu_unit()) };
    let gpu = d.gpu();
    let gpu = if gpu == "—" { gpu } else { format!("{gpu}°C") };
    let rows = [
        ("PUMP", pump, d.pump_rpm.map(|_| d.pump_frac())),
        ("FAN", fan, d.fan_percent.map(|_| d.fan_frac())),
        ("CPU", cpu, d.has_cpu().then(|| d.cpu_frac())),
        ("GPU", gpu, d.gpu_temp_c.map(|_| d.gpu_frac())),
    ];
    // 名称 56px，间距 12px，条，间距 12px，数值 64px；20 格之间 3px。
    let (bar_x, bar_w) = (PAD + 56.0 + 12.0, 480.0 - 2.0 * PAD - 56.0 - 64.0 - 24.0);
    let cell_w = (bar_w - 3.0 * (CELLS - 1) as f32) / CELLS as f32;
    let lit: Vec<usize> = rows.iter().map(|r| r.2.map_or(0, |f| (f * CELLS as f32).round() as usize)).collect();
    let label = TextStyle::mono(15.0);

    let draw_text = |l: &mut Canvas| {
        l.text("COOLANT·水温", PAD, HEAD_BASE, &head, acc);
        l.text(&clock, clock_x, HEAD_BASE, &head.right(), acc);
        for (i, (name, value, _)) in rows.iter().enumerate() {
            let base = ROWS_TOP + i as f32 * ROW_STEP + 13.9;
            l.text(name, PAD, base, &label, acc);
            l.text(value, 480.0 - PAD, base, &label.right(), acc);
        }
    };
    let draw_lit = |l: &mut Canvas| {
        for (i, &n) in lit.iter().enumerate() {
            let y = ROWS_TOP + i as f32 * ROW_STEP + 0.7;
            for k in 0..n {
                l.fill_round_rect(bar_x + k as f32 * (cell_w + 3.0), y, cell_w, 16.0, 1.0, acc);
            }
        }
    };
    // 文字和点亮的格子共用一层辉光（CSS 里是 6–8px 的 text-shadow / box-shadow）。
    c.glow(3.5, 1.0, |l| {
        draw_text(l);
        draw_lit(l);
    });
    for (i, &n) in lit.iter().enumerate() {
        let y = ROWS_TOP + i as f32 * ROW_STEP + 0.7;
        for k in n..CELLS {
            c.fill_round_rect(bar_x + k as f32 * (cell_w + 3.0), y, cell_w, 16.0, 1.0, alpha(acc, 24.0 / 255.0));
        }
    }
    draw_lit(c);
    draw_text(c);

    // ---- 扫描线：每 3px 一条 1px 的暗线 ----
    for k in 0..160 {
        c.fill_rect(0.0, k as f32 * 3.0 + 2.0, 480.0, 1.0, rgba(0x000000, 0.28));
    }

    // ---- 大读数（定位元素，盖在扫描线上面）----
    let big = TextStyle::num(156.0, 700).spacing_em(0.02);
    let base = BIG_TOP + 0.794 * 156.0;
    c.text("88.8", PAD, base, &big, alpha(acc, 20.0 / 255.0));
    let value = d.coolant();
    let draw_value = |l: &mut Canvas| draw_digits(l, &value, base, &big, acc);
    c.glow(9.0, 1.0, draw_value);
    c.glow(1.0, 1.0, draw_value);
    draw_value(c);

    let unit = TextStyle::mono(28.0).right();
    let unit_base = BIG_TOP + 14.0 + 0.928 * 28.0;
    c.glow(5.0, 1.0, |l| {
        l.text("°C", 480.0 - PAD - 4.0, unit_base, &unit, acc);
    });
    c.text("°C", 480.0 - PAD - 4.0, unit_base, &unit, acc);
}

/// 大读数。和“88.8”残影同样是“两位.一位”时逐字对齐到残影的格子里（Bahnschrift 的“1”比别的数字窄，
/// 整串左对齐会和残影错开）；其它情况（缺失、三位数）整串左对齐。
fn draw_digits(c: &mut Canvas, value: &str, base: f32, big: &TextStyle, col: Rgba) {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() != 4 || chars[2] != '.' {
        c.text(value, PAD, base, big, col);
        return;
    }
    let ghost: Vec<char> = "88.8".chars().collect();
    let mut x = PAD;
    for (ch, g) in chars.iter().zip(&ghost) {
        let cell = c.measure(&g.to_string(), big);
        let w = c.measure(&ch.to_string(), big);
        // 数字在格子里靠右，和段码屏一样。
        c.text(&ch.to_string(), x + cell - w, base, big, col);
        x += cell + big.spacing;
    }
}

/// 百分比；缺失时不带 %。
fn pct(v: &str) -> String {
    if v == "—" {
        v.to_string()
    } else {
        format!("{v}%")
    }
}
