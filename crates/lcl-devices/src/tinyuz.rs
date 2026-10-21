//! tinyuz 压缩（vendor/ 下的 C++ 库，经 vendor/tuz_wrapper.cpp 暴露为 C 接口）。
//! 风扇和冷头固件用 tinyuz 解压灯效帧数据。字典固定 4 KB，与 lian-li-linux 一致。

use std::os::raw::c_uchar;

extern "C" {
    fn tuz_compress_mem(
        input: *const c_uchar,
        input_len: usize,
        output: *mut c_uchar,
        output_capacity: usize,
        dict_size: usize,
    ) -> usize;
    fn tuz_max_compressed_size(input_len: usize) -> usize;
}

#[cfg(test)]
extern "C" {
    fn tuz_decompress_mem(in_code: *const c_uchar, code_size: usize, out_data: *mut c_uchar, data_size: *mut usize) -> u32;
}

const DICT_SIZE: usize = 4096;

pub fn compress(input: &[u8]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(!input.is_empty(), "tinyuz：输入为空");
    // SAFETY: 纯函数，只按长度计算上限。
    let max = unsafe { tuz_max_compressed_size(input.len()) };
    let mut out = vec![0u8; max];
    // SAFETY: 输入、输出缓冲区及其长度都来自有效的切片。
    let n = unsafe { tuz_compress_mem(input.as_ptr(), input.len(), out.as_mut_ptr(), out.len(), DICT_SIZE) };
    anyhow::ensure!(n != 0, "tinyuz：压缩失败");
    out.truncate(n);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decompress(code: &[u8], len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        let mut size = len;
        // SAFETY: 缓冲区大小与 size 一致。
        let r = unsafe { tuz_decompress_mem(code.as_ptr(), code.len(), out.as_mut_ptr(), &mut size) };
        assert_eq!(r, 1, "解压失败：{r}");
        assert_eq!(size, len);
        out
    }

    #[test]
    fn round_trips_led_frames() {
        // 3 个 TL 风扇 × 26 LED × 30 帧的渐变。
        let raw: Vec<u8> = (0..30u8)
            .flat_map(|f| (0..78u8).flat_map(move |i| [f.wrapping_mul(8), i.wrapping_mul(3), 200]))
            .collect();
        let code = compress(&raw).unwrap();
        assert!(code.len() < raw.len());
        assert_eq!(decompress(&code, raw.len()), raw);
    }

    #[test]
    fn rejects_empty_input() {
        assert!(compress(&[]).is_err());
    }
}
