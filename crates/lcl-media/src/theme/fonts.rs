//! 主题用的系统字体：数字用 Bahnschrift（DIN 风格，可变字重），中文用微软雅黑，等宽用 Cascadia Mono。
//! 字体文件内存映射、整个进程只加载一次（和仪表盘一样，映射的页按需加载、不算私有内存）。

use ab_glyph::{Font, FontRef, GlyphId, VariableFont};
use std::sync::OnceLock;

/// 字族。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Bahnschrift：读数、时间。
    Num,
    /// 微软雅黑：中文和说明文字。
    Ui,
    /// Cascadia Mono：复古液晶之类的等宽标签。
    Mono,
}

pub struct Fonts {
    /// Bahnschrift 的五档字重：300 / 400 / 500 / 600 / 700。
    num: Vec<FontRef<'static>>,
    /// 微软雅黑细 / 常规 / 粗。
    ui: [FontRef<'static>; 3],
    mono: FontRef<'static>,
}

fn map(path: &str) -> Option<&'static [u8]> {
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: 系统字体文件在运行期间不会被改写；映射只读。
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    Some(&Box::leak(Box::new(map))[..])
}

fn load(paths: &[&str]) -> Option<FontRef<'static>> {
    paths.iter().find_map(|p| map(p).and_then(|b| FontRef::try_from_slice_and_index(b, 0).ok()))
}

const FONTS: &str = r"C:\Windows\Fonts\";

impl Fonts {
    fn load() -> Option<Self> {
        let f = |name: &str| format!("{FONTS}{name}");
        let regular = load(&[&f("msyh.ttc"), &f("segoeui.ttf")])?;
        let ui = [
            load(&[&f("msyhl.ttc")]).unwrap_or_else(|| regular.clone()),
            regular.clone(),
            load(&[&f("msyhbd.ttc")]).unwrap_or_else(|| regular.clone()),
        ];
        let bahn = load(&[&f("bahnschrift.ttf")]);
        let num = [300.0, 400.0, 500.0, 600.0, 700.0]
            .iter()
            .map(|&w| match &bahn {
                Some(b) => {
                    let mut font = b.clone();
                    font.set_variation(b"wght", w);
                    font
                }
                None => regular.clone(),
            })
            .collect();
        let mono = load(&[&f("CascadiaMono.ttf"), &f("consola.ttf")]).unwrap_or_else(|| regular.clone());
        Some(Self { num, ui, mono })
    }

    /// 按字族和字重（100–900）选字体。
    pub fn pick(&self, family: Family, weight: u16) -> &FontRef<'static> {
        match family {
            Family::Num => &self.num[(((weight.clamp(300, 700) - 300) + 50) / 100) as usize],
            Family::Ui => &self.ui[if weight <= 300 { 0 } else if weight >= 600 { 2 } else { 1 }],
            Family::Mono => &self.mono,
        }
    }

    /// 字体里没有这个字（比如 Bahnschrift 没有中文）就退回微软雅黑。
    pub fn glyph(&self, family: Family, weight: u16, ch: char) -> (&FontRef<'static>, GlyphId) {
        let font = self.pick(family, weight);
        let id = font.glyph_id(ch);
        if id.0 != 0 || family == Family::Ui {
            return (font, id);
        }
        let fb = self.pick(Family::Ui, weight);
        (fb, fb.glyph_id(ch))
    }
}

/// 全局字体；系统里一个可用字体都没有时返回 None。
pub fn fonts() -> Option<&'static Fonts> {
    static FONTS: OnceLock<Option<Fonts>> = OnceLock::new();
    FONTS.get_or_init(Fonts::load).as_ref()
}
