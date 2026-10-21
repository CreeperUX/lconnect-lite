//! 调试：像后台服务一样向 lcl-sensord 要 5 次 CPU 读数。
//! `cargo run -p lcl-core --example cpu_sensors`

fn main() {
    for _ in 0..5 {
        println!("{:?}", lcl_core::sensors::read());
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
