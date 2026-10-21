//! 灯效动画的打包：无线风扇用 RF 0x20 分包上传，冷头灯圈用 USB 0xFC 一次推送。
//! 设备没有内置效果，主机把每一帧每颗 LED 的颜色算好、tinyuz 压缩后下发，设备循环播放。
//! 包格式移植自 lian-li-linux `wireless/rgb.rs`、`winusb/h2_aio.rs`（MIT）。

use crate::tinyuz;
use anyhow::{ensure, Context, Result};

/// 官方上传器的上限（MasterDevice.LzoMaxRgbDataLen）。
const MAX_COMPRESSED_BYTES: usize = 12_288;
pub const MAX_FRAMES: usize = 4096;
/// 无线数据包每包携带的压缩数据字节数。
pub(crate) const CHUNK_BYTES: usize = 220;

/// 一段打包好的无线灯效。同样的动画算出同样的 `effect_index`，重复上传是幂等的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbUpload {
    pub(crate) compressed: Vec<u8>,
    pub(crate) led_count: u8,
    pub(crate) frame_count: u16,
    pub(crate) interval_ticks: u16,
    pub(crate) interval_fraction: u8,
    pub effect_index: [u8; 4],
}

impl RgbUpload {
    /// `frames`：每帧每颗 LED 的 RGB；`interval_hundredths`：帧间隔，单位为百分之一个 0.625 ms tick。
    pub fn new(frames: &[Vec<[u8; 3]>], interval_hundredths: u32) -> Result<Self> {
        ensure!(!frames.is_empty() && frames.len() <= MAX_FRAMES, "灯效帧数须为 1..={MAX_FRAMES}");
        let led_count = frames[0].len();
        ensure!((1..=255).contains(&led_count), "每帧 LED 数须为 1..=255");
        ensure!(frames.iter().all(|f| f.len() == led_count), "各帧 LED 数不一致");
        ensure!((100..=6_553_599).contains(&interval_hundredths), "帧间隔超出范围");

        let raw: Vec<u8> = frames.iter().flatten().flatten().copied().collect();
        let compressed = tinyuz::compress(&raw).context("压缩灯效")?;
        ensure!(compressed.len() <= MAX_COMPRESSED_BYTES, "压缩后超过 {MAX_COMPRESSED_BYTES} 字节，减少帧数");

        let interval_ticks = (interval_hundredths / 100) as u16;
        let interval_fraction = (interval_hundredths % 100) as u8;
        let frame_count = frames.len() as u16;
        // FNV-1a，覆盖内容和时序（次级时序字段 TL 不用，按 0 参与计算，与 lian-li-linux 一致）。
        let mut hash = 0x811c_9dc5u32;
        for byte in compressed
            .iter()
            .copied()
            .chain(interval_ticks.to_be_bytes())
            .chain([interval_fraction])
            .chain(frame_count.to_be_bytes())
            .chain([led_count as u8])
            .chain([0, 0, 0, 0, 0])
        {
            hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
        }
        // 把灯效包残留字节误当成地址的设备会把第一个字节读成 rx 槽位；置最高位让它无效。
        let mut effect_index = hash.to_be_bytes();
        effect_index[0] |= 0x80;
        Ok(Self {
            compressed,
            led_count: led_count as u8,
            frame_count,
            interval_ticks,
            interval_fraction,
            effect_index,
        })
    }

    /// 头包 1 个 + 数据包若干。
    pub fn packet_count(&self) -> usize {
        self.compressed.len().div_ceil(CHUNK_BYTES) + 1
    }

    /// 第 `index` 个 240 字节 RF 包。
    pub(crate) fn packet(&self, mac: &[u8; 6], master: &[u8; 6], index: usize) -> Vec<u8> {
        let mut p = vec![0u8; 240];
        p[0] = 0x12;
        p[1] = 0x20;
        p[2..8].copy_from_slice(mac);
        p[8..14].copy_from_slice(master);
        p[14..18].copy_from_slice(&self.effect_index);
        p[18] = index as u8;
        p[19] = self.packet_count() as u8;
        if index == 0 {
            p[20..24].copy_from_slice(&(self.compressed.len() as u32).to_be_bytes());
            p[25..27].copy_from_slice(&self.frame_count.to_be_bytes());
            p[27] = self.led_count;
            p[32..34].copy_from_slice(&self.interval_ticks.to_be_bytes());
            p[34] = self.interval_fraction;
        } else {
            let start = (index - 1) * CHUNK_BYTES;
            let data = &self.compressed[start..self.compressed.len().min(start + CHUNK_BYTES)];
            p[20..20 + data.len()].copy_from_slice(data);
        }
        p
    }
}

/// 冷头灯圈 24 颗 LED 的 0xFC 载荷：tinyuz(帧×24×RGB) + 帧数 u16 BE + 间隔 tick（u8）+ LED 数。
pub fn h2_ring_payload(frames: &[Vec<[u8; 3]>], interval_ticks: u8) -> Result<Vec<u8>> {
    const RING: usize = 24;
    ensure!(!frames.is_empty(), "灯效至少一帧");
    ensure!(interval_ticks > 0, "帧间隔至少 1 tick");
    let frame_count = u16::try_from(frames.len()).context("帧数过多")?;
    let raw: Vec<u8> = frames
        .iter()
        .flat_map(|f| (0..RING).map(move |i| f.get(i).copied().unwrap_or([0; 3])))
        .flatten()
        .collect();
    let mut payload = tinyuz::compress(&raw).context("压缩冷头灯效")?;
    payload.extend_from_slice(&frame_count.to_be_bytes());
    payload.push(interval_ticks);
    payload.push(RING as u8);
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(n: usize, leds: usize) -> Vec<Vec<[u8; 3]>> {
        (0..n).map(|f| (0..leds).map(|i| [f as u8, i as u8, 200]).collect()).collect()
    }

    #[test]
    fn header_and_data_packets_follow_the_vendor_layout() {
        let up = RgbUpload::new(&frames(30, 78), 33 * 100).unwrap();
        let h = up.packet(&[1; 6], &[2; 6], 0);
        assert_eq!(&h[..2], &[0x12, 0x20]);
        assert_eq!(&h[14..18], &up.effect_index);
        assert_eq!(h[19] as usize, up.packet_count());
        assert_eq!(u32::from_be_bytes(h[20..24].try_into().unwrap()) as usize, up.compressed.len());
        assert_eq!(u16::from_be_bytes([h[25], h[26]]), 30);
        assert_eq!(h[27], 78);
        assert_eq!(u16::from_be_bytes([h[32], h[33]]), 33);
        let d = up.packet(&[1; 6], &[2; 6], 1);
        assert_eq!(d[18], 1);
        let n = up.compressed.len().min(CHUNK_BYTES);
        assert_eq!(&d[20..20 + n], &up.compressed[..n]);
        assert!(up.effect_index[0] & 0x80 != 0);
    }

    #[test]
    fn effect_index_is_deterministic_and_content_sensitive() {
        let a = RgbUpload::new(&frames(10, 78), 2200).unwrap();
        let b = RgbUpload::new(&frames(10, 78), 2200).unwrap();
        let c = RgbUpload::new(&frames(10, 78), 2300).unwrap();
        assert_eq!(a.effect_index, b.effect_index);
        assert_ne!(a.effect_index, c.effect_index);
    }

    #[test]
    fn ring_payload_trailer() {
        let p = h2_ring_payload(&frames(5, 24), 40).unwrap();
        assert_eq!(&p[p.len() - 4..], &[0, 5, 40, 24]);
    }
}
