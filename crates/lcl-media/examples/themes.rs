//! 把所有主题按示例数据渲染成 PNG，对照设计稿检查：
//! `cargo run -p lcl-media --example themes -- <输出目录> [主题 id …]`
//! 每套输出 `<id>-dark.png`，有亮色的再输出 `<id>-light.png`，另外拼一张总览 `all.png`。

use image::{imageops, RgbImage};
use lcl_media::theme::{render, rgb, Style, ThemeData, THEMES};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "theme-previews".into()));
    let only: Vec<String> = args.collect();
    std::fs::create_dir_all(&dir)?;
    let data = ThemeData::sample();
    let mut shots = Vec::new();
    for t in THEMES.iter().filter(|t| only.is_empty() || only.iter().any(|o| o == t.id)) {
        for dark in [true, false] {
            if !dark && !t.light {
                continue;
            }
            let started = std::time::Instant::now();
            let img = render(t.id, &data, &Style { dark, accent: rgb(t.accent) });
            let ms = started.elapsed().as_millis();
            let path = dir.join(format!("{}-{}.png", t.id, if dark { "dark" } else { "light" }));
            img.save(&path)?;
            println!("{}  {} ms", path.display(), ms);
            shots.push(img);
        }
    }
    // 总览：每行 6 张，缩到一半。
    let cols = 6u32;
    let rows = (shots.len() as u32).div_ceil(cols).max(1);
    let mut sheet = RgbImage::from_pixel(cols * 250, rows * 250, image::Rgb([24, 24, 24]));
    for (i, img) in shots.iter().enumerate() {
        let small = imageops::resize(img, 240, 240, imageops::FilterType::Triangle);
        imageops::replace(&mut sheet, &small, i64::from(i as u32 % cols * 250 + 5), i64::from(i as u32 / cols * 250 + 5));
    }
    sheet.save(dir.join("all.png"))?;
    Ok(())
}
