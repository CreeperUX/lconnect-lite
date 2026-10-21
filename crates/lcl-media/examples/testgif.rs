//! 生成一个测试用 GIF（彩色色块转圈）：`cargo run -p lcl-media --example testgif -- out.gif`

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, Rgba, RgbaImage};

fn main() -> anyhow::Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "test.gif".into());
    let mut enc = GifEncoder::new(std::fs::File::create(&out)?);
    enc.set_repeat(Repeat::Infinite)?;
    for i in 0..24u32 {
        let a = i as f32 / 24.0 * std::f32::consts::TAU;
        let img = RgbaImage::from_fn(240, 240, |x, y| {
            let (cx, cy) = (120.0 + 70.0 * a.cos(), 120.0 + 70.0 * a.sin());
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            if d < 30.0 {
                Rgba([242, 163, 58, 255])
            } else {
                Rgba([15, 17, 19, 255])
            }
        });
        enc.encode_frame(Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(60, 1)))?;
    }
    println!("{out}");
    Ok(())
}
