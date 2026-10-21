//! 冷头屏幕（480×480）的画面准备：图片和 GIF 裁成正方形、旋转、压成 JPEG；
//! 以及传感器仪表盘的渲染。屏幕本身不会旋转，旋转都在这里做。

mod dashboard;

pub use dashboard::{Dashboard, DashboardData};

use anyhow::{Context, Result};
use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, DynamicImage, RgbImage};
use std::io::BufReader;
use std::path::Path;

pub const SIZE: u32 = 480;
/// 单帧 JPEG 上限（设备接收缓冲）。
pub const MAX_JPEG: usize = 153_600;
/// GIF 最多取这么多帧，避免长动图占太多内存。
pub const MAX_GIF_FRAMES: usize = 300;

/// 一帧画面和它在动画里停留的时间。
pub struct Frame {
    pub jpeg: Vec<u8>,
    pub delay_ms: u32,
}

/// 居中裁成正方形再缩放到 480×480。
pub fn fit_square(img: &DynamicImage) -> RgbImage {
    let (w, h) = (img.width(), img.height());
    let side = w.min(h);
    let cropped = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
    cropped.resize_exact(SIZE, SIZE, FilterType::Lanczos3).to_rgb8()
}

/// 按 0/90/180/270 度顺时针旋转。
pub fn rotate(img: RgbImage, degrees: u16) -> RgbImage {
    match degrees % 360 {
        90 => image::imageops::rotate90(&img),
        180 => image::imageops::rotate180(&img),
        270 => image::imageops::rotate270(&img),
        _ => img,
    }
}

/// 旋转后编码成 JPEG；超过设备上限就逐步降低质量。
pub fn encode(img: RgbImage, rotation: u16) -> Result<Vec<u8>> {
    let img = rotate(img, rotation);
    for quality in [85u8, 75, 65, 50, 35] {
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, quality).encode_image(&img)?;
        if out.len() <= MAX_JPEG {
            return Ok(out);
        }
    }
    anyhow::bail!("画面过于复杂，压缩到最低质量仍超过 {MAX_JPEG} 字节")
}

/// 读一张静态图片（png/jpg/bmp/webp，GIF 只取第一帧）。
pub fn load_still(path: &Path, rotation: u16) -> Result<Vec<u8>> {
    let img = image::open(path).with_context(|| format!("打开图片 {}", path.display()))?;
    encode(fit_square(&img), rotation)
}

/// 把 GIF 拆成 JPEG 帧。帧间隔不小于 1000/max_fps 毫秒（太快的帧合并延时，不丢画面节奏）。
pub fn load_gif(path: &Path, rotation: u16, max_fps: u8) -> Result<Vec<Frame>> {
    let file = std::fs::File::open(path).with_context(|| format!("打开 {}", path.display()))?;
    let decoder = GifDecoder::new(BufReader::new(file)).context("不是有效的 GIF")?;
    let min_delay = 1000 / u32::from(max_fps.max(1));
    let mut frames: Vec<Frame> = Vec::new();
    let mut carry = 0u32;
    for frame in decoder.into_frames().take(MAX_GIF_FRAMES) {
        let frame = frame.context("解码 GIF 帧")?;
        let (num, den) = frame.delay().numer_denom_ms();
        let delay = if den == 0 { 100 } else { num / den }.max(20) + carry;
        // 间隔太短的帧跳过，把时间累加到下一帧上。
        if delay < min_delay && !frames.is_empty() {
            carry = delay;
            continue;
        }
        carry = 0;
        let img = DynamicImage::ImageRgba8(frame.into_buffer());
        frames.push(Frame { jpeg: encode(fit_square(&img), rotation)?, delay_ms: delay });
    }
    anyhow::ensure!(!frames.is_empty(), "GIF 里没有帧");
    Ok(frames)
}

/// 纯色画面（比如“关屏”时推一张黑图）。
pub fn solid(rgb: [u8; 3]) -> Result<Vec<u8>> {
    encode(RgbImage::from_pixel(SIZE, SIZE, image::Rgb(rgb)), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_to_square_and_encodes_within_limit() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(800, 450, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])));
        let sq = fit_square(&img);
        assert_eq!(sq.dimensions(), (SIZE, SIZE));
        let jpeg = encode(sq, 90).unwrap();
        assert!(jpeg.len() <= MAX_JPEG);
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((back.width(), back.height()), (SIZE, SIZE));
    }

    #[test]
    fn rotation_moves_pixels_clockwise() {
        let mut img = RgbImage::new(SIZE, SIZE);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        assert_eq!(rotate(img, 90).get_pixel(SIZE - 1, 0).0, [255, 0, 0]);
    }
}
