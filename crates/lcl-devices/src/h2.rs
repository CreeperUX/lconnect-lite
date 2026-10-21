//! 隐流二代（HydroShift II LCD）冷头的 USB 控制通道。
//! 协议移植自 lian-li-linux 的 crypto.rs / winusb/h2_aio.rs（MIT）。

use crate::usb::UsbDev;
use anyhow::{Context, Result};
use cbc::Encryptor;
use des::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use des::Des;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// (VID, PID)：圆屏 LCD-C、方屏 LCD-S。
pub const H2_IDS: [(u16, u16); 2] = [(0x1CBE, 0xA021), (0x1CBE, 0xA034)];
const PID_SQUARE: u16 = 0xA034;

const DES_KEY: &[u8; 8] = b"slv3tuzx";

const CMD_GET_VER: u8 = 0x0A;
const CMD_BRIGHTNESS: u8 = 0x0E;
const CMD_FRAME_RATE: u8 = 0x0F;
const CMD_SET_CLOCK: u8 = 0x33;
const CMD_STOP_CLOCK: u8 = 0x34;
const CMD_PUSH_JPG: u8 = 0x65;
/// PNG 叠加层，画在 JPEG 层上面（官方主题的文字、仪表都画在这层）。
const CMD_PUSH_PNG: u8 = 0x66;
const CMD_QUERY_BLOCK: u8 = 0x7A;
const CMD_STOP_PLAY: u8 = 0x7B;

/// 屏幕：480×480，单帧 JPEG 最大字节数。
pub const LCD_SIZE: u32 = 480;
pub const LCD_MAX_JPEG: usize = 153_600;
/// 推图要给足写超时：写到一半超时会让固件失步，只能断电恢复。
const FRAME_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// 离线时钟（SetClock 0x33 的模式字节，来自 lianli.lcd207 的 SyncClock）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockMode {
    Off = 0,
    On = 1,
    /// 只同步时间，不改开关。
    SyncOnly = 2,
}
const CMD_GET_H2_PARAMS: u8 = 0xFA;
const CMD_SYNC_PUMP_FAN: u8 = 0xFB;
const CMD_PUSH_RGB: u8 = 0xFC;

const WRITE_TIMEOUT: Duration = Duration::from_millis(200);
const READ_TIMEOUT: Duration = Duration::from_millis(2000);

pub const PUMP_MIN_RPM: u16 = 1600;

#[derive(Debug, Clone)]
pub struct H2Params {
    pub pump_rpm: u16,
    pub fan_rpm: [u16; 3],
    pub coolant_c: u8,
    /// 冷头自己的无线 MAC；无线模式下 RX 设备列表里用它找到冷头。
    pub mac: Option<[u8; 6]>,
}

pub struct H2 {
    usb: UsbDev,
    started: Instant,
    last_ts: u32,
    pub square: bool,
}

impl H2 {
    pub fn open() -> Result<Option<Self>> {
        Ok(UsbDev::open_any(&H2_IDS, "隐流二代冷头")?.map(|usb| Self {
            square: usb.pid == PID_SQUARE,
            usb,
            started: Instant::now(),
            last_ts: 0,
        }))
    }

    pub fn max_pump_rpm(&self) -> u16 {
        if self.square {
            3200
        } else {
            2500
        }
    }

    /// 读泵速、水温、冷排风扇转速。LCD 处于播放模式时设备会忽略命令，
    /// 第一次读不到就发 StopPlay → StopClock → GetVer 唤醒后再试。
    pub fn get_params(&mut self) -> Result<H2Params> {
        match self.try_get_params() {
            Ok(p) => Ok(p),
            Err(_) => {
                self.wake();
                self.try_get_params()
            }
        }
    }

    fn try_get_params(&mut self) -> Result<H2Params> {
        let mut buf = [0u8; 512];
        for _ in 0..2 {
            self.usb.flush();
            let frame = self.frame(CMD_GET_H2_PARAMS, &[]);
            self.usb.write(&frame, WRITE_TIMEOUT).context("写 GetH2Params")?;
            if self.read_reply(CMD_GET_H2_PARAMS, &mut buf, READ_TIMEOUT)? >= 32 {
                return Ok(parse_params(&buf));
            }
        }
        anyhow::bail!("没有收到 GetH2Params 回复")
    }

    /// 回复的第 0 字节是对应的命令号。0xFB 的回复可能晚到几百毫秒，
    /// 不按命令号匹配就会被当成 0xFA 的结果（泵速、水温全读成 0）。
    fn read_reply(&self, cmd: u8, buf: &mut [u8; 512], timeout: Duration) -> Result<usize> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(0);
            }
            let n = self.usb.read(buf, left)?;
            // 冷头会插入零长度包；读到 0 字节不代表超时，继续等到截止时间。
            if n == 0 {
                continue;
            }
            if buf[0] == cmd {
                return Ok(n);
            }
            if std::env::var_os("LCL_DEBUG").is_some() {
                eprintln!("    [debug] 等 0x{cmd:02X} 时丢掉一条 0x{:02X} 回复", buf[0]);
            }
        }
    }

    /// 设泵和三个冷排风扇。`fans` 是原始 0-255 占空比。
    pub fn sync_pump_fan(&mut self, pump_rpm: u16, fans: [u8; 3]) -> Result<()> {
        let rpm = pump_rpm.clamp(PUMP_MIN_RPM, self.max_pump_rpm());
        let frame = self.frame(
            CMD_SYNC_PUMP_FAN,
            &sync_pump_fan_payload(pump_timer(rpm, self.square), fans),
        );
        self.usb.write(&frame, WRITE_TIMEOUT).context("写 SyncPumpFan")?;
        // 等到自己的回复再返回，免得它迟到后混进下一次读取。
        let mut buf = [0u8; 512];
        self.read_reply(CMD_SYNC_PUMP_FAN, &mut buf, Duration::from_millis(1000))?;
        Ok(())
    }

    // ---------- 屏幕 ----------

    /// 屏幕初始化：停止设备自己的播放、帧率 30、同步时间、关掉屏幕上的时钟层，之后才能推图。
    /// 冷头每次重新打开（包括推送灯圈之后）都要重做。
    pub fn lcd_init(&mut self) -> Result<()> {
        self.lcd_command(CMD_STOP_PLAY, &[])?;
        self.lcd_command(CMD_GET_VER, &[])?;
        self.lcd_command(CMD_FRAME_RATE, &[30])?;
        self.set_clock(ClockMode::SyncOnly)?;
        self.lcd_command(CMD_STOP_CLOCK, &[0])?;
        Ok(())
    }

    /// 亮度 0-100。
    pub fn set_brightness(&mut self, percent: u8) -> Result<()> {
        self.lcd_command(CMD_BRIGHTNESS, &[percent.min(100)])
    }

    /// 写入当前本地时间并设置离线时钟（电脑没在推图时屏幕显示时钟）。
    pub fn set_clock(&mut self, mode: ClockMode) -> Result<()> {
        let mut p = local_time_bytes();
        p[7] = mode as u8;
        self.lcd_command(CMD_SET_CLOCK, &p)
    }

    /// 推一帧 JPEG（必须是 480×480、不超过 `LCD_MAX_JPEG`）。设备缓冲水位高于 3 时
    /// 等到降到 2 以下再返回，调用方按自己的节奏推下一帧即可。
    pub fn push_jpeg(&mut self, jpeg: &[u8]) -> Result<()> {
        self.push_layer(CMD_PUSH_JPG, jpeg).context("推送 JPEG")
    }

    /// 写 PNG 叠加层。设备会一直保留这层（包括 L-Connect 主题留下的），
    /// 推一张全透明的 PNG 才能清掉，否则会盖在 JPEG 画面上。
    pub fn push_png(&mut self, png: &[u8]) -> Result<()> {
        self.push_layer(CMD_PUSH_PNG, png).context("推送 PNG 叠加层")
    }

    fn push_layer(&mut self, cmd: u8, data: &[u8]) -> Result<()> {
        anyhow::ensure!(data.len() <= LCD_MAX_JPEG, "图片太大（{} 字节）", data.len());
        let mut packet = self.frame(cmd, &(data.len() as u32).to_be_bytes()).to_vec();
        packet.extend_from_slice(data);
        self.usb.write(&packet, FRAME_WRITE_TIMEOUT)?;
        let mut buf = [0u8; 512];
        let n = self.read_reply(cmd, &mut buf, READ_TIMEOUT)?;
        if n > 8 && buf[8] > 3 {
            self.wait_buffer(2)?;
        }
        Ok(())
    }

    /// 诊断用：发一条命令（可带载荷），返回原样收到的回复（不按命令号匹配）。
    pub fn raw_exchange(&mut self, cmd: u8, params: &[u8], payload: &[u8], wait: Duration) -> Result<Vec<Vec<u8>>> {
        let mut packet = self.frame(cmd, params).to_vec();
        packet.extend_from_slice(payload);
        self.usb.write(&packet, FRAME_WRITE_TIMEOUT)?;
        let mut replies = Vec::new();
        let deadline = Instant::now() + wait;
        let mut buf = [0u8; 512];
        while Instant::now() < deadline {
            let n = self.usb.read(&mut buf, deadline.saturating_duration_since(Instant::now()))?;
            replies.push(buf[..n].to_vec());
        }
        Ok(replies)
    }

    /// 每 50 ms 查一次缓冲水位（QueryBlock 0x7A），直到不高于 `threshold`，最多等 30 秒。
    fn wait_buffer(&mut self, threshold: u8) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut buf = [0u8; 512];
        while Instant::now() < deadline {
            let frame = self.frame(CMD_QUERY_BLOCK, &[]);
            self.usb.write(&frame, WRITE_TIMEOUT)?;
            let n = self.read_reply(CMD_QUERY_BLOCK, &mut buf, Duration::from_millis(500))?;
            if n <= 8 || buf[8] <= threshold {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        anyhow::bail!("屏幕缓冲 30 秒没有排空")
    }

    /// 发一条屏幕控制命令并读掉它的回复。
    fn lcd_command(&mut self, cmd: u8, params: &[u8]) -> Result<()> {
        let frame = self.frame(cmd, params);
        self.usb.write(&frame, WRITE_TIMEOUT).with_context(|| format!("屏幕命令 0x{cmd:02X}"))?;
        let mut buf = [0u8; 512];
        let _ = self.read_reply(cmd, &mut buf, Duration::from_millis(500))?;
        Ok(())
    }

    /// 推送灯圈动画（0xFC）。推完冷头会停止应答，必须：先 StopPlay/StopClock → 一次写完头和载荷
    /// → 关闭重开 USB → 轮询 GetVer 直到应答，整个过程限时 3 秒（lian-li-linux 实测约 3.1 秒恢复）。
    /// 返回后调用方应尽快重发一次泵速设定。
    pub fn push_ring(&mut self, frames: &[Vec<[u8; 3]>], interval_ticks: u8) -> Result<()> {
        let payload = crate::rgb::h2_ring_payload(frames, interval_ticks)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut buf = [0u8; 512];
        for cmd in [CMD_STOP_PLAY, CMD_STOP_CLOCK] {
            let frame = self.frame(cmd, &[]);
            self.usb.write(&frame, WRITE_TIMEOUT)?;
            let _ = self.usb.read(&mut buf, Duration::from_millis(100));
        }
        let mut params = [0u8; 8];
        params[4..8].copy_from_slice(&(payload.len() as u32).to_be_bytes());
        let mut packet = self.frame(CMD_PUSH_RGB, &params).to_vec();
        packet.extend_from_slice(&payload);
        // 大包写入必须给足时间：写到一半超时会让固件失步，只能断电恢复。
        let written = self.usb.write(&packet, Duration::from_secs(2));
        let _ = self.usb.read(&mut buf, Duration::from_millis(100));

        self.usb.reopen("隐流二代冷头")?;
        self.usb.flush();
        loop {
            anyhow::ensure!(Instant::now() < deadline, "冷头推送灯效后 3 秒内没有恢复应答");
            let frame = self.frame(CMD_GET_VER, &[]);
            self.usb.write(&frame, WRITE_TIMEOUT)?;
            let n = self.usb.read(&mut buf, Duration::from_millis(500)).unwrap_or(0);
            if valid_version_reply(&buf[..n]) {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        written.context("冷头灯效写入不完整（设备已恢复）")
    }

    fn wake(&mut self) {
        for cmd in [CMD_STOP_PLAY, CMD_STOP_CLOCK, CMD_GET_VER] {
            let frame = self.frame(cmd, &[]);
            let _ = self.usb.write(&frame, WRITE_TIMEOUT);
            let mut buf = [0u8; 512];
            let _ = self.usb.read(&mut buf, READ_TIMEOUT);
            sleep(Duration::from_millis(150));
        }
    }

    fn frame(&mut self, cmd: u8, params: &[u8]) -> [u8; 512] {
        let raw = self.started.elapsed().as_millis() as u32;
        self.last_ts = raw.max(self.last_ts + 1);
        build_frame(cmd, self.last_ts, params)
    }
}

/// 512 字节命令帧：前 500 字节 `[cmd, 0, 0x1A, 0x6D, 时间戳 LE, 参数…]`
/// 用 DES-CBC（key = IV = "slv3tuzx"）加密，末尾两字节固定 `A1 1A`。
fn build_frame(cmd: u8, timestamp: u32, params: &[u8]) -> [u8; 512] {
    let mut plain = [0u8; 508];
    plain[0] = cmd;
    plain[2] = 0x1A;
    plain[3] = 0x6D;
    plain[4..8].copy_from_slice(&timestamp.to_le_bytes());
    let n = params.len().min(492);
    plain[8..8 + n].copy_from_slice(&params[..n]);

    let encrypted = Encryptor::<Des>::new_from_slices(DES_KEY, DES_KEY)
        .expect("DES key/IV 都是 8 字节")
        .encrypt_padded_mut::<Pkcs7>(&mut plain, 500)
        .expect("508 字节缓冲区足够放下填充");

    let mut out = [0u8; 512];
    out[..504].copy_from_slice(encrypted);
    out[510] = 0xA1;
    out[511] = 0x1A;
    out
}

/// `[年高, 年低, 月, 日, 时, 分, 秒, 模式]`，本地时间。
fn local_time_bytes() -> [u8; 8] {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: GetLocalTime 只写入传入的结构体。
    let t = unsafe {
        let mut t: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    let y = t.wYear.to_be_bytes();
    [y[0], y[1], t.wMonth as u8, t.wDay as u8, t.wHour as u8, t.wMinute as u8, t.wSecond as u8, 0]
}

/// GetVer 回复：第 0 字节 0x0A，8..40 是可打印的版本字符串。
fn valid_version_reply(reply: &[u8]) -> bool {
    reply.first() == Some(&CMD_GET_VER)
        && reply.get(8..40).is_some_and(|v| {
            let text = v.split(|&b| b == 0).next().unwrap_or_default();
            !text.is_empty() && text.iter().all(|b| b.is_ascii_graphic() || *b == b' ')
        })
}

fn sync_pump_fan_payload(pump_timer: u16, fans: [u8; 3]) -> [u8; 16] {
    let mut p = [0u8; 16];
    p[..4].copy_from_slice(&[0xFF, 0x0F, 0xA2, 0x00]);
    p[4..6].copy_from_slice(&pump_timer.to_be_bytes());
    p[6..9].copy_from_slice(&fans);
    let crc = crc16_xmodem(&p[..12]);
    p[12..14].copy_from_slice(&crc.to_be_bytes());
    p
}

fn parse_params(buf: &[u8; 512]) -> H2Params {
    let be = |i: usize| u16::from_be_bytes([buf[i], buf[i + 1]]);
    let mac: [u8; 6] = buf[22..28].try_into().unwrap();
    H2Params {
        pump_rpm: be(20),
        fan_rpm: [be(14), be(16), be(18)],
        coolant_c: buf[13],
        mac: (mac != [0; 6]).then_some(mac),
    }
}

/// CRC-16/XMODEM（多项式 0x1021，初值 0）。
fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// 目标泵速 → 固件定时器值（越小越快）。分段表来自官方软件，
/// USB 和无线两条路径共用。
pub fn pump_timer(rpm: u16, square: bool) -> u16 {
    let t = if square {
        let rpm = rpm.clamp(1600, 3200) as f32;
        if rpm <= 1800.0 {
            1590.0 - (rpm - 1600.0) * 0.95
        } else if rpm <= 2000.0 {
            1400.0 - (rpm - 1800.0)
        } else if rpm <= 2200.0 {
            1200.0 - (rpm - 2000.0)
        } else if rpm <= 2400.0 {
            1000.0 - (rpm - 2200.0)
        } else if rpm <= 2600.0 {
            800.0 - (rpm - 2400.0)
        } else if rpm <= 2800.0 {
            580.0 - (rpm - 2600.0) * 1.11
        } else if rpm <= 3000.0 {
            330.0 - (rpm - 2800.0) * 1.2
        } else {
            90.0 - (rpm - 3000.0) * 0.45
        }
    } else {
        let rpm = rpm.clamp(1600, 2500) as f32;
        if rpm <= 1720.0 {
            1500.0 - (rpm - 1600.0) * 1.667
        } else if rpm <= 1870.0 {
            1300.0 - (rpm - 1720.0) * 2.0
        } else if rpm <= 2000.0 {
            1000.0 - (rpm - 1870.0) * 1.23
        } else if rpm <= 2300.0 {
            840.0 - (rpm - 2000.0) * 2.0
        } else if rpm <= 2400.0 {
            240.0 - (rpm - 2300.0) * 1.8
        } else {
            60.0 - (rpm - 2400.0) * 0.5
        }
    };
    t.round().clamp(0.0, u16::MAX as f32) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::Decryptor;
    use des::cipher::BlockDecryptMut;

    #[test]
    fn crc_matches_xmodem_check_value() {
        assert_eq!(crc16_xmodem(b"123456789"), 0x31C3);
    }

    #[test]
    fn frame_decrypts_back_to_header_and_params() {
        let frame = build_frame(CMD_SYNC_PUMP_FAN, 0x0102_0304, &[0xAA, 0xBB]);
        assert_eq!(&frame[510..], &[0xA1, 0x1A]);
        assert!(frame[504..510].iter().all(|&b| b == 0));
        let mut enc = frame[..504].to_vec();
        let plain = Decryptor::<Des>::new_from_slices(DES_KEY, DES_KEY)
            .unwrap()
            .decrypt_padded_mut::<Pkcs7>(&mut enc)
            .unwrap();
        assert_eq!(plain.len(), 500);
        assert_eq!(&plain[..10], &[0xFB, 0, 0x1A, 0x6D, 4, 3, 2, 1, 0xAA, 0xBB]);
    }

    #[test]
    fn sync_payload_layout() {
        let p = sync_pump_fan_payload(0x0123, [10, 20, 30]);
        assert_eq!(&p[..9], &[0xFF, 0x0F, 0xA2, 0, 0x01, 0x23, 10, 20, 30]);
        assert_eq!(
            u16::from_be_bytes([p[12], p[13]]),
            crc16_xmodem(&p[..12])
        );
    }

    #[test]
    fn pump_timer_segments() {
        assert_eq!(pump_timer(1600, true), 1590);
        assert_eq!(pump_timer(3200, true), 0);
        assert_eq!(pump_timer(1000, true), pump_timer(1600, true));
        assert_eq!(pump_timer(1600, false), 1500);
        assert_eq!(pump_timer(5000, false), pump_timer(2500, false));
    }
}
