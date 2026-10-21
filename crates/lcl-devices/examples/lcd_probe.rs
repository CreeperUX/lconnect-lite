//! 屏幕协议诊断：初始化后连续推几帧 JPEG，打印每条命令的原始回复，并在推帧间隙读 0xFA。
//! 用法（后台服务必须先停）：`cargo run -p lcl-devices --example lcd_probe -- frame.jpg`

use lcl_devices::h2::H2;
use std::time::{Duration, Instant};

fn hex(b: &[u8]) -> String {
    b.iter().take(16).map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

fn main() -> anyhow::Result<()> {
    let jpeg = std::fs::read(std::env::args().nth(1).expect("给一个 JPEG 文件"))?;
    let mut h2 = H2::open()?.expect("没找到冷头");
    h2.lcd_init()?;
    for i in 0..8 {
        let t = Instant::now();
        let r = h2.raw_exchange(0x65, &(jpeg.len() as u32).to_be_bytes(), &jpeg, Duration::from_millis(300))?;
        println!("帧 {i}: {} ms, {} 条回复", t.elapsed().as_millis(), r.len());
        for x in &r {
            println!("    {} 字节: {}", x.len(), hex(x));
        }
        let t = Instant::now();
        match h2.get_params() {
            Ok(p) => println!("  0xFA OK {} ms：水温 {} 泵 {}", t.elapsed().as_millis(), p.coolant_c, p.pump_rpm),
            Err(e) => println!("  0xFA 失败 {} ms：{e:#}", t.elapsed().as_millis()),
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    let r = h2.raw_exchange(0x7A, &[], &[], Duration::from_millis(300))?;
    for x in &r {
        println!("QueryBlock: {} 字节: {}", x.len(), hex(x));
    }
    Ok(())
}
