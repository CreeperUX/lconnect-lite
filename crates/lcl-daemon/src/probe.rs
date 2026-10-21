//! 调试用的直连命令（`probe` / `run`）：不经过后台服务，直接打开设备。
//! 后台服务运行时设备被它独占，这两个命令会打不开设备。

use anyhow::{bail, Result};
use lcl_devices::h2::{H2Params, H2, PUMP_MIN_RPM};
use lcl_devices::wireless::{aio_params, fmt_mac, RfDevice, Snapshot, Wireless};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// 冷头 USB 控制的刷新间隔；固件在更频繁的写入下会卡死（官方也是约 3.6 秒）。
const H2_SYNC_INTERVAL: Duration = Duration::from_millis(3600);
const H2_READ_INTERVAL: Duration = Duration::from_secs(2);
/// 和官方一样每秒推一次。lian-li-linux 说风扇停收 PWM 会回到默认转速，
/// 但实测 TL 无线在官方服务停掉几分钟后仍保持最后的 PWM。
const TICK: Duration = Duration::from_secs(1);
const MIN_FAN_PERCENT: u8 = 20;

pub fn status() -> Result<()> {
    let mut h2 = H2::open()?;
    let mut wl = Wireless::connect(false)?;
    if h2.is_none() && wl.is_none() {
        bail!("没有找到隐流二代冷头，也没有找到无线接收器");
    }

    let mut h2_mac = None;
    if let Some(h2) = h2.as_mut() {
        println!("隐流二代冷头（USB，{}）", if h2.square { "方屏" } else { "圆屏" });
        let p = h2.get_params()?;
        print_h2(&p);
        h2_mac = p.mac;
    }

    if let Some(wl) = wl.as_mut() {
        println!(
            "\n无线发射器  MAC {}  频道 {}  固件 {}",
            fmt_mac(&wl.master),
            wl.channel,
            wl.tx_firmware
        );
        let snap = poll_until_devices(wl)?;
        match snap.mb_pwm {
            Some(d) => println!("RX 采样到的主板 PWM：{}%", u32::from(d) * 100 / 255),
            None => println!("RX 没有采样到主板 PWM 信号"),
        }
        for d in &snap.devices {
            print_rf_device(d, &wl.master);
        }
        let h2_wireless = wl.own_aio(&snap).is_some();
        if h2.is_some() {
            println!(
                "\n冷头当前控制方式：{}",
                if h2_wireless {
                    "无线（已绑定本机发射器，USB 只用来读数据）"
                } else {
                    "USB"
                }
            );
        }
        if let Some(mac) = h2_mac {
            if !snap.devices.iter().any(|d| d.mac == mac) {
                println!("（冷头的无线 MAC {} 不在 RX 列表里）", fmt_mac(&mac));
            }
        }
    }
    Ok(())
}

pub fn run(pump_rpm: u16, fan_pct: u8, limit: Option<Duration>, dry_run: bool) -> Result<()> {
    let mut h2 = H2::open()?;
    let mut wl = Wireless::connect(dry_run)?;
    if h2.is_none() && wl.is_none() {
        bail!("没有找到隐流二代冷头，也没有找到无线接收器");
    }

    let fan_pct = fan_pct.clamp(MIN_FAN_PERCENT, 100);
    let fan_duty = (u16::from(fan_pct) * 255 / 100) as u8;
    let square = h2.as_ref().map_or(true, |h| h.square);
    let max_pump = if square { 3200 } else { 2500 };
    let pump_rpm = pump_rpm.clamp(PUMP_MIN_RPM, max_pump);
    println!("目标：泵 {pump_rpm} rpm，风扇 {fan_pct}%{}。Ctrl+C 退出。", if dry_run { "（dry-run）" } else { "" });

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        ctrlc::set_handler(move || stop.store(true, Ordering::SeqCst))?;
    }

    let started = Instant::now();
    let mut last_h2_sync: Option<Instant> = None;
    let mut last_h2_read: Option<Instant> = None;
    let mut h2_params: Option<H2Params> = None;

    while !stop.load(Ordering::SeqCst) && limit.map_or(true, |l| started.elapsed() < l) {
        let tick = Instant::now();
        let mut line = Vec::new();
        let mut h2_wireless = false;

        if let Some(wl) = wl.as_mut() {
            // 偶尔会读到不是 GetDev 的回复，丢掉这一轮、继续用缓存，不中断保活。
            let snap = wl.poll().unwrap_or_else(|e| {
                line.push(format!("（RX 本轮跳过：{e:#}）"));
                wl.cached()
            });
            wl.send_clock()?;
            if let Some((slot, aio)) = wl.own_aio(&snap) {
                h2_wireless = true;
                let aio = aio.clone();
                // 和后台服务一样：泵只由参数块控制，PWM 帧只给带无线风扇的冷头发，泵那一路不动。
                wl.set_aio_params(&aio, &aio_params(pump_rpm, square))?;
                if aio.fan_count > 0 {
                    wl.set_pwm(&aio, slot, [fan_duty, fan_duty, fan_duty, aio.pwm[3]])?;
                }
                line.push(format!(
                    "泵 {} rpm 水温 {}°C 冷排 {:?}",
                    aio.pump_rpm().unwrap_or(0),
                    aio.coolant_c().map_or("?".into(), |c| c.to_string()),
                    &aio.rpm[..usize::from(aio.fan_count.min(3))]
                ));
            }
            for (slot, dev) in wl.own_devices(&snap) {
                if dev.is_aio() {
                    continue;
                }
                let dev = dev.clone();
                wl.set_pwm(&dev, slot, [fan_duty; 4])?;
                line.push(format!(
                    "{} {:?}",
                    dev.kind(),
                    &dev.rpm[..usize::from(dev.fan_count)]
                ));
            }
        }

        if let Some(h2) = h2.as_mut() {
            if !h2_wireless {
                let due = last_h2_sync.map_or(true, |t| t.elapsed() >= H2_SYNC_INTERVAL);
                if due && !dry_run {
                    h2.sync_pump_fan(pump_rpm, [fan_duty; 3])?;
                    last_h2_sync = Some(Instant::now());
                }
            }
            if last_h2_read.map_or(true, |t| t.elapsed() >= H2_READ_INTERVAL) {
                h2_params = Some(h2.get_params()?);
                last_h2_read = Some(Instant::now());
            }
            if let (Some(p), false) = (&h2_params, h2_wireless) {
                line.insert(
                    0,
                    format!(
                        "泵 {} rpm 水温 {}°C 冷排 {:?}",
                        p.pump_rpm, p.coolant_c, p.fan_rpm
                    ),
                );
            }
        }

        println!("[{:>4}s] {}", started.elapsed().as_secs(), line.join(" | "));
        sleep(TICK.saturating_sub(tick.elapsed()));
    }

    println!("已停止。设备保持最后收到的转速；重新启动 LConnectService 可恢复官方控制。");
    Ok(())
}

/// RX 刚打开时第一次 GetDev 可能还是空的，多问几次。
fn poll_until_devices(wl: &mut Wireless) -> Result<Snapshot> {
    let mut snap = wl.poll()?;
    for _ in 0..4 {
        if !snap.devices.is_empty() {
            break;
        }
        sleep(Duration::from_millis(300));
        snap = wl.poll()?;
    }
    Ok(snap)
}

fn print_h2(p: &H2Params) {
    println!("  泵速   {} rpm", p.pump_rpm);
    println!("  水温   {} °C", p.coolant_c);
    println!("  冷排风扇 {:?} rpm", p.fan_rpm);
    if let Some(mac) = p.mac {
        println!("  无线 MAC {}", fmt_mac(&mac));
    }
}

fn print_rf_device(d: &RfDevice, master: &[u8; 6]) {
    let owner = if &d.master == master {
        "本机"
    } else if d.master == [0; 6] {
        "未绑定"
    } else {
        "其他发射器"
    };
    let mut extra = String::new();
    if let (Some(pump), Some(c)) = (d.pump_rpm(), d.coolant_c()) {
        extra = format!("  泵 {pump} rpm  水温 {c}°C");
    }
    println!(
        "  {}  {:<12} {:<6} 风扇 {}  转速 {:?}  PWM {:?}  频道 {} rx {}{}",
        d.mac_str(),
        d.kind(),
        owner,
        d.fan_count,
        &d.rpm[..usize::from(d.fan_count.min(4))],
        d.pwm,
        d.channel,
        d.rx_type,
        extra
    );
}
