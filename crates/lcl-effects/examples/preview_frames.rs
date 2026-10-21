//! 导出灯效预览帧（界面原型用）：`cargo run -p lcl-effects --example preview_frames -- out.json`
//!
//! 每种效果用真实渲染器算出整段动画，再均匀抽样到最多 `MAX_FRAMES` 帧。
//! 输出：`{"fan":{"<模式>":{"ms":帧间隔毫秒,"frames":["rrggbb…",…]}},"pump":{…}}`，
//! 风扇是 3 只 × 26 颗灯珠，冷头灯圈 24 颗。

use lcl_effects::types::{RgbEffectParameters, RgbRegionConfig};
use lcl_effects::{Animation, RgbEffect, RgbMode};
use std::fmt::Write as _;

const FANS: usize = 3;
const MAX_FRAMES: usize = 72;
const COLORS: [[u8; 3]; 4] = [[242, 163, 58], [74, 164, 255], [255, 90, 110], [91, 224, 122]];

fn effect(mode: RgbMode, p: &RgbEffectParameters) -> RgbEffect {
    let n = usize::from(p.max_colors.max(p.min_colors)).min(COLORS.len());
    RgbEffect { mode, colors: COLORS[..n].to_vec(), speed: 2, brightness: 4, ..RgbEffect::default() }
}

fn encode(anim: &Animation, out: &mut String) {
    let (ms, frames) = anim.preview_hex(MAX_FRAMES);
    write!(out, "{{\"ms\":{ms:.1},\"frames\":[").unwrap();
    for (i, frame) in frames.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write!(out, "\"{frame}\"").unwrap();
    }
    out.push_str("]}");
}

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "preview_frames.json".into());
    let mut out = String::from("{\"fan\":{");
    let mut first = true;
    for p in lcl_effects::tl::parameters::for_fans(FANS as u8) {
        if p.mode == RgbMode::Off {
            continue;
        }
        let region = RgbRegionConfig { effect: effect(p.mode, &p), flip: false };
        let anim = match lcl_effects::tl::render(&[region], FANS) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("跳过风扇 {:?}：{e:#}", p.mode);
                continue;
            }
        };
        if !first {
            out.push(',');
        }
        first = false;
        write!(out, "\"{:?}\":", p.mode).unwrap();
        encode(&anim, &mut out);
    }
    out.push_str("},\"pump\":{");
    first = true;
    for p in lcl_effects::h2::parameters() {
        if p.mode == RgbMode::Off {
            continue;
        }
        let anim = lcl_effects::h2::render(&effect(p.mode, &p), 24)?;
        if !first {
            out.push(',');
        }
        first = false;
        write!(out, "\"{:?}\":", p.mode).unwrap();
        encode(&anim, &mut out);
    }
    out.push_str("}}");
    std::fs::write(&path, &out)?;
    println!("{path}：{} KB", out.len() / 1024);
    Ok(())
}
