//! 主题绘图工具：在 tiny-skia 之上包一层，坐标单位是屏幕像素（480×480），角度单位是度。
//!
//! 角度约定：0° 指向右（3 点钟），顺时针为正（屏幕坐标 y 向下）。
//! 例如 135° 是左下，270° 是正上方。

use super::fonts::{fonts, Family};
use ab_glyph::{Font, OutlineCurve, ScaleFont};
use image::RgbImage;
use tiny_skia::{
    FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Mask, Path, PathBuilder, Pixmap, PixmapPaint, Point, RadialGradient, Rect,
    Shader, SpreadMode, Stroke, Transform,
};

/// 非预乘的 RGBA。
pub type Rgba = [u8; 4];

/// `0xRRGGBB` → 不透明颜色。
pub const fn rgb(hex: u32) -> Rgba {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255]
}

/// `0xRRGGBB` 加透明度（0–1）。
pub fn rgba(hex: u32, alpha: f32) -> Rgba {
    let [r, g, b, _] = rgb(hex);
    [r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8]
}

/// 给已有颜色换透明度。
pub fn alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], (a.clamp(0.0, 1.0) * 255.0).round() as u8]
}

/// 线性混合两个颜色，t=0 是 a，t=1 是 b。
pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|i| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t).round() as u8)
}

/// 填充或描边用的颜料：纯色、线性渐变、径向渐变。渐变的 stop 位置是 0–1。
#[derive(Clone, Debug)]
pub enum Paint {
    Solid(Rgba),
    Linear { from: (f32, f32), to: (f32, f32), stops: Vec<(f32, Rgba)> },
    Radial { center: (f32, f32), radius: f32, stops: Vec<(f32, Rgba)> },
}

impl From<Rgba> for Paint {
    fn from(c: Rgba) -> Self {
        Paint::Solid(c)
    }
}

impl Paint {
    pub fn linear(from: (f32, f32), to: (f32, f32), stops: &[(f32, Rgba)]) -> Self {
        Paint::Linear { from, to, stops: stops.to_vec() }
    }
    pub fn radial(center: (f32, f32), radius: f32, stops: &[(f32, Rgba)]) -> Self {
        Paint::Radial { center, radius, stops: stops.to_vec() }
    }

    fn shader(&self) -> Shader<'static> {
        let color = |c: Rgba| tiny_skia::Color::from_rgba8(c[0], c[1], c[2], c[3]);
        let stops = |s: &[(f32, Rgba)]| s.iter().map(|&(p, c)| GradientStop::new(p, color(c))).collect::<Vec<_>>();
        match self {
            Paint::Solid(c) => Shader::SolidColor(color(*c)),
            Paint::Linear { from, to, stops: s } => {
                LinearGradient::new(Point::from_xy(from.0, from.1), Point::from_xy(to.0, to.1), stops(s), SpreadMode::Pad, Transform::identity())
                    .unwrap_or(Shader::SolidColor(color(s[0].1)))
            }
            Paint::Radial { center, radius, stops: s } => {
                let c = Point::from_xy(center.0, center.1);
                RadialGradient::new(c, c, *radius, stops(s), SpreadMode::Pad, Transform::identity()).unwrap_or(Shader::SolidColor(color(s[0].1)))
            }
        }
    }

    fn paint(&self) -> tiny_skia::Paint<'static> {
        tiny_skia::Paint { shader: self.shader(), anti_alias: true, ..Default::default() }
    }
}

/// 文字对齐：x 是左边、中点还是右边。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// 文字样式。`size` 和 CSS 的 font-size 一样（em 大小，像素）。
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub family: Family,
    pub size: f32,
    /// 100–900；Bahnschrift 支持 300–700 连续字重，雅黑只有细 / 常规 / 粗三档。
    pub weight: u16,
    pub align: Align,
    /// 额外字距（像素），可以为负。
    pub spacing: f32,
}

impl TextStyle {
    /// 数字（Bahnschrift）。
    pub fn num(size: f32, weight: u16) -> Self {
        Self { family: Family::Num, size, weight, align: Align::Left, spacing: 0.0 }
    }
    /// 中文 / 说明文字（微软雅黑）。
    pub fn ui(size: f32) -> Self {
        Self { family: Family::Ui, size, weight: 400, align: Align::Left, spacing: 0.0 }
    }
    /// 等宽（Cascadia Mono）。
    pub fn mono(size: f32) -> Self {
        Self { family: Family::Mono, size, weight: 400, align: Align::Left, spacing: 0.0 }
    }
    pub fn weight(mut self, w: u16) -> Self {
        self.weight = w;
        self
    }
    pub fn center(mut self) -> Self {
        self.align = Align::Center;
        self
    }
    pub fn right(mut self) -> Self {
        self.align = Align::Right;
        self
    }
    /// 字距，单位是 em 的比例（和 CSS 的 letter-spacing: 0.1em 一样）。
    pub fn spacing_em(mut self, em: f32) -> Self {
        self.spacing = em * self.size;
        self
    }
}

pub struct Canvas {
    pm: Pixmap,
}

impl Canvas {
    /// 透明画布。
    pub fn new(width: u32, height: u32) -> Self {
        Self { pm: Pixmap::new(width, height).expect("画布尺寸非零") }
    }

    pub fn width(&self) -> f32 {
        self.pm.width() as f32
    }
    pub fn height(&self) -> f32 {
        self.pm.height() as f32
    }

    // ---------- 基本图形 ----------

    /// 整张画布填一层（会叠在已有内容上，用于背景）。
    pub fn fill_all(&mut self, paint: impl Into<Paint>) {
        let (w, h) = (self.width(), self.height());
        self.fill_rect(0.0, 0.0, w, h, paint);
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, paint: impl Into<Paint>) {
        if let Some(r) = Rect::from_xywh(x, y, w, h) {
            self.pm.fill_rect(r, &paint.into().paint(), Transform::identity(), None);
        }
    }

    pub fn fill_round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, paint: impl Into<Paint>) {
        if let Some(p) = round_rect(x, y, w, h, r) {
            self.fill_path(&p, paint);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, width: f32, paint: impl Into<Paint>) {
        if let Some(p) = round_rect(x, y, w, h, r) {
            self.stroke_path(&p, width, false, paint);
        }
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, paint: impl Into<Paint>) {
        if let Some(p) = PathBuilder::from_circle(cx, cy, r) {
            self.fill_path(&p, paint);
        }
    }

    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, width: f32, paint: impl Into<Paint>) {
        if let Some(p) = PathBuilder::from_circle(cx, cy, r) {
            self.stroke_path(&p, width, false, paint);
        }
    }

    /// 圆弧：从 `start` 度顺时针扫 `sweep` 度。`round_cap` 是圆头。
    #[allow(clippy::too_many_arguments)]
    pub fn stroke_arc(&mut self, cx: f32, cy: f32, r: f32, start: f32, sweep: f32, width: f32, round_cap: bool, paint: impl Into<Paint>) {
        if sweep.abs() < 0.01 {
            return;
        }
        if let Some(p) = arc_path(cx, cy, r, start, sweep) {
            self.stroke_path(&p, width, round_cap, paint);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, round_cap: bool, paint: impl Into<Paint>) {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        if let Some(p) = pb.finish() {
            self.stroke_path(&p, width, round_cap, paint);
        }
    }

    /// 折线（圆角连接）。
    pub fn stroke_polyline(&mut self, pts: &[(f32, f32)], width: f32, round_cap: bool, paint: impl Into<Paint>) {
        if let Some(p) = polyline(pts, false) {
            self.stroke_path(&p, width, round_cap, paint);
        }
    }

    pub fn fill_polygon(&mut self, pts: &[(f32, f32)], paint: impl Into<Paint>) {
        if let Some(p) = polyline(pts, true) {
            self.fill_path(&p, paint);
        }
    }

    /// 任意路径填充（非零环绕规则）。复杂形状用 [`PathBuilder`] 自己拼。
    pub fn fill_path(&mut self, path: &Path, paint: impl Into<Paint>) {
        self.pm.fill_path(path, &paint.into().paint(), FillRule::Winding, Transform::identity(), None);
    }

    pub fn stroke_path(&mut self, path: &Path, width: f32, round_cap: bool, paint: impl Into<Paint>) {
        let stroke = Stroke {
            width,
            line_cap: if round_cap { LineCap::Round } else { LineCap::Butt },
            line_join: LineJoin::Round,
            ..Default::default()
        };
        self.pm.stroke_path(path, &paint.into().paint(), &stroke, Transform::identity(), None);
    }

    // ---------- 图层与效果 ----------

    /// 在独立的透明图层上画，再以 `opacity` 叠回来（整组半透明时用）。
    pub fn layer(&mut self, opacity: f32, draw: impl FnOnce(&mut Canvas)) {
        let mut l = Canvas::new(self.pm.width(), self.pm.height());
        draw(&mut l);
        self.composite(&l, opacity);
    }

    /// 辉光：把 `draw` 画的东西模糊 `radius` 像素后以 `opacity` 叠上来。
    /// 只画光晕，不画清晰的本体——本体要另外再画一遍。
    pub fn glow(&mut self, radius: f32, opacity: f32, draw: impl FnOnce(&mut Canvas)) {
        let mut l = Canvas::new(self.pm.width(), self.pm.height());
        draw(&mut l);
        l.blur(radius);
        self.composite(&l, opacity);
    }

    /// 只在 `clip` 路径里面画（比如圆形容器里的液面）。
    pub fn clipped(&mut self, clip: &Path, draw: impl FnOnce(&mut Canvas)) {
        let mut l = Canvas::new(self.pm.width(), self.pm.height());
        draw(&mut l);
        if let Some(mut mask) = Mask::new(self.pm.width(), self.pm.height()) {
            mask.fill_path(clip, FillRule::Winding, true, Transform::identity());
            l.pm.apply_mask(&mask);
        }
        self.composite(&l, 1.0);
    }

    /// 毛玻璃：把当前画面在圆角矩形范围内模糊 `blur` 像素后贴回去（背景虚化），再叠一层 `tint`。
    #[allow(clippy::too_many_arguments)]
    pub fn frosted(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, blur: f32, tint: impl Into<Paint>) {
        let Some(shape) = round_rect(x, y, w, h, r) else { return };
        let mut copy = Canvas { pm: self.pm.clone() };
        copy.blur(blur);
        if let Some(mut mask) = Mask::new(self.pm.width(), self.pm.height()) {
            mask.fill_path(&shape, FillRule::Winding, true, Transform::identity());
            copy.pm.apply_mask(&mask);
        }
        self.composite(&copy, 1.0);
        self.fill_path(&shape, tint);
    }

    /// 细噪点（胶片颗粒），让大面积暗色/渐变不显得廉价。`strength` 0–1，常用 0.03–0.08。
    pub fn noise(&mut self, strength: f32, seed: u32) {
        let w = self.pm.width();
        let amp = strength * 255.0;
        for (i, px) in self.pm.data_mut().chunks_exact_mut(4).enumerate() {
            let (x, y) = (i as u32 % w, i as u32 / w);
            let n = hash(x, y, seed) - 0.5;
            let max = f32::from(px[3]);
            let a = max / 255.0;
            for c in &mut px[..3] {
                // 预乘颜色：噪点幅度也按 alpha 缩放，透明处不加。
                *c = (f32::from(*c) + n * amp * a).clamp(0.0, max) as u8;
            }
        }
    }

    /// 高斯近似模糊（三次盒式模糊），半径约等于 CSS blur() 的数值。
    pub fn blur(&mut self, radius: f32) {
        if radius < 0.5 {
            return;
        }
        let (w, h) = (self.pm.width() as usize, self.pm.height() as usize);
        // 三次盒式模糊逼近 sigma = radius 的高斯。
        let boxes = boxes_for_gauss(radius, 3);
        let data = self.pm.data_mut();
        let mut tmp = vec![0u8; data.len()];
        for r in boxes {
            box_blur_h(data, &mut tmp, w, h, r);
            box_blur_v(&tmp, data, w, h, r);
        }
    }

    fn composite(&mut self, other: &Canvas, opacity: f32) {
        let paint = PixmapPaint { opacity: opacity.clamp(0.0, 1.0), ..Default::default() };
        self.pm.draw_pixmap(0, 0, other.pm.as_ref(), &paint, Transform::identity(), None);
    }

    // ---------- 文字 ----------

    /// 量文字宽度。
    pub fn measure(&self, text: &str, style: &TextStyle) -> f32 {
        layout(text, style).map_or(0.0, |(_, w)| w)
    }

    /// 写一行字，`y` 是基线。返回宽度。颜料可以是渐变（按画布坐标）。
    pub fn text(&mut self, text: &str, x: f32, y: f32, style: &TextStyle, paint: impl Into<Paint>) -> f32 {
        let Some((glyphs, width)) = layout(text, style) else { return 0.0 };
        let x0 = match style.align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        };
        let mut pb = PathBuilder::new();
        for g in &glyphs {
            append_glyph(&mut pb, g, x0, y);
        }
        if let Some(p) = pb.finish() {
            self.fill_path(&p, paint);
        }
        width
    }

    // ---------- 输出 ----------

    /// 合成到黑底上，转成 RGB 图像。
    pub fn into_rgb(self) -> RgbImage {
        let (w, h) = (self.pm.width(), self.pm.height());
        // 预乘数据叠在黑底上，结果就是预乘后的 RGB。
        let data = self.pm.data().chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        RgbImage::from_raw(w, h, data).expect("尺寸一致")
    }
}

/// 排好版的一个字形。
struct Placed {
    font: &'static ab_glyph::FontRef<'static>,
    id: ab_glyph::GlyphId,
    x: f32,
    sx: f32,
    sy: f32,
}

fn layout(text: &str, style: &TextStyle) -> Option<(Vec<Placed>, f32)> {
    let fonts = fonts()?;
    let mut out = Vec::new();
    let mut caret = 0.0;
    let mut prev: Option<(&'static ab_glyph::FontRef<'static>, ab_glyph::GlyphId, char)> = None;
    for ch in text.chars() {
        let (font, id) = fonts.glyph(style.family, style.weight, ch);
        // 和 CSS 一致：size 是 em 大小。ab_glyph 的 PxScale 以 (上升 - 下降) 为单位，换算一下。
        let px = style.size * font.height_unscaled() / font.units_per_em().unwrap_or(1000.0);
        let scaled = font.as_scaled(px);
        if let Some((pf, pid, pch)) = prev {
            if std::ptr::eq(pf, font) {
                caret += scaled.kern(pid, id);
                if style.family == Family::Num && std::ptr::eq(font, fonts.pick(Family::Num, style.weight)) {
                    caret += bahn_kern(pch, ch) * style.size / 1000.0;
                }
            }
        }
        let f = scaled.scale_factor();
        out.push(Placed { font, id, x: caret, sx: f.horizontal, sy: f.vertical });
        caret += scaled.h_advance(id) + style.spacing;
        prev = Some((font, id, ch));
    }
    // 最后一个字后面的字距不算宽度。
    let width = if out.is_empty() { 0.0 } else { caret - style.spacing };
    Some((out, width))
}

/// Bahnschrift 的字偶距放在 GPOS 里，ab_glyph 只读旧式 kern 表，读不到；
/// 不补的话大号数字里“.0”“74”这类组合比设计稿（浏览器排版）松好几个像素。
/// 这里是在浏览器里量出来的值（千分之一 em，各字重相同）。只列数字、标点和 C 之间不为零的组合。
fn bahn_kern(a: char, b: char) -> f32 {
    match (a, b) {
        ('0' | 'C', '0' | 'C') => -20.0,
        ('0' | 'C', '3') => -5.0,
        ('0' | 'C', '.' | ':') | ('.' | ':', '0' | 'C') | ('2', '4' | '6') => -29.0,
        ('4', '1') => -24.0,
        ('7' | '9', '4' | '6') => -39.0,
        ('—', '1') => -15.0,
        ('—', '7') => -34.0,
        _ => 0.0,
    }
}

fn append_glyph(pb: &mut PathBuilder, g: &Placed, x0: f32, baseline: f32) {
    let Some(outline) = g.font.outline(g.id) else { return };
    let tx = |p: ab_glyph::Point| (x0 + g.x + p.x * g.sx, baseline - p.y * g.sy);
    let mut last: Option<(f32, f32)> = None;
    for curve in &outline.curves {
        let (start, end) = match curve {
            OutlineCurve::Line(a, b) => (*a, *b),
            OutlineCurve::Quad(a, _, b) => (*a, *b),
            OutlineCurve::Cubic(a, _, _, b) => (*a, *b),
        };
        let s = tx(start);
        // 曲线不连续就是新的轮廓。
        if last.is_none_or(|l| (l.0 - s.0).abs() > 0.01 || (l.1 - s.1).abs() > 0.01) {
            if last.is_some() {
                pb.close();
            }
            pb.move_to(s.0, s.1);
        }
        match curve {
            OutlineCurve::Line(_, b) => {
                let e = tx(*b);
                pb.line_to(e.0, e.1);
            }
            OutlineCurve::Quad(_, c, b) => {
                let (c, e) = (tx(*c), tx(*b));
                pb.quad_to(c.0, c.1, e.0, e.1);
            }
            OutlineCurve::Cubic(_, c1, c2, b) => {
                let (c1, c2, e) = (tx(*c1), tx(*c2), tx(*b));
                pb.cubic_to(c1.0, c1.1, c2.0, c2.1, e.0, e.1);
            }
        }
        last = Some(tx(end));
    }
    if last.is_some() {
        pb.close();
    }
}

/// 圆角矩形路径。
pub fn round_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    // 用三次贝塞尔近似四分之一圆。
    let k = 0.552_284_8 * r;
    let mut pb = PathBuilder::new();
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
    pb.finish()
}

/// 圆弧路径（分段三次贝塞尔，每段不超过 90°）。
pub fn arc_path(cx: f32, cy: f32, r: f32, start: f32, sweep: f32) -> Option<Path> {
    let segs = (sweep.abs() / 90.0).ceil().max(1.0) as usize;
    let step = sweep.to_radians() / segs as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut a = start.to_radians();
    let mut pb = PathBuilder::new();
    pb.move_to(cx + r * a.cos(), cy + r * a.sin());
    for _ in 0..segs {
        let b = a + step;
        let (x0, y0) = (cx + r * a.cos(), cy + r * a.sin());
        let (x3, y3) = (cx + r * b.cos(), cy + r * b.sin());
        pb.cubic_to(x0 - k * r * a.sin(), y0 + k * r * a.cos(), x3 + k * r * b.sin(), y3 - k * r * b.cos(), x3, y3);
        a = b;
    }
    pb.finish()
}

/// 圆周上 `deg` 度处的点。
pub fn polar(cx: f32, cy: f32, r: f32, deg: f32) -> (f32, f32) {
    let a = deg.to_radians();
    (cx + r * a.cos(), cy + r * a.sin())
}

fn polyline(pts: &[(f32, f32)], close: bool) -> Option<Path> {
    let (first, rest) = pts.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.0, first.1);
    for p in rest {
        pb.line_to(p.0, p.1);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

/// 可复现的伪随机数（0–1），噪点和装饰用。
pub fn hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65535.0
}

fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<usize> {
    let ideal = ((12.0 * sigma * sigma / n as f32) + 1.0).sqrt();
    let mut wl = ideal.floor() as usize;
    if wl.is_multiple_of(2) {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - (n * wl * wl) as f32 - (4 * n * wl) as f32 - (3 * n) as f32) / (-4.0 * wl as f32 - 4.0)).round() as usize;
    (0..n).map(|i| (if i < m { wl } else { wu } - 1) / 2).collect()
}

fn box_blur_h(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let div = (2 * r + 1) as u32;
    for y in 0..h {
        let row = y * w * 4;
        for c in 0..4 {
            let at = |x: isize| u32::from(src[row + (x.clamp(0, w as isize - 1) as usize) * 4 + c]);
            let mut acc: u32 = (-(r as isize)..=r as isize).map(at).sum();
            for x in 0..w {
                dst[row + x * 4 + c] = (acc / div) as u8;
                acc = acc + at(x as isize + r as isize + 1) - at(x as isize - r as isize);
            }
        }
    }
}

fn box_blur_v(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let div = (2 * r + 1) as u32;
    for x in 0..w {
        for c in 0..4 {
            let at = |y: isize| u32::from(src[(y.clamp(0, h as isize - 1) as usize) * w * 4 + x * 4 + c]);
            let mut acc: u32 = (-(r as isize)..=r as isize).map(at).sum();
            for y in 0..h {
                dst[y * w * 4 + x * 4 + c] = (acc / div) as u8;
                acc = acc + at(y as isize + r as isize + 1) - at(y as isize - r as isize);
            }
        }
    }
}
