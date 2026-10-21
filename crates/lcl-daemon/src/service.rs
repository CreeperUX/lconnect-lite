//! 后台服务：独占设备，每秒一轮控制循环，状态和配置通过命名管道给 GUI。

use crate::cpu::CpuUsage;
use crate::h2slot::H2Slot;
use crate::lcd;
use crate::lighting::LightingCtl;
use lcl_core::gpu::Gpu;
use lcl_core::ipc::LightingCaps;
use crate::{autostart, log, tray};
use anyhow::{Context, Result};
use lcl_core::config::{boost, CoolingMode, FanOverride, TempSource, MIN_FAN_PERCENT, PUMP_MIN_RPM};
use lcl_core::{ipc, Config, DeviceInfo, FanGroup, Request, Response, Status, WirelessDevice};
use std::sync::mpsc;
use lcl_devices::h2::{H2Params, H2};
use lcl_devices::wireless::{aio_params, fmt_mac, Snapshot, Wireless};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_secs(1);
/// 冷头固件在更频繁的 0xFB 写入下会卡死，官方也是约 3.6 秒一次。
const H2_SYNC_INTERVAL: Duration = Duration::from_millis(3600);
const H2_READ_INTERVAL: Duration = Duration::from_secs(2);
/// 设备没插或打开失败后，隔多久再试。
const REOPEN_INTERVAL: Duration = Duration::from_secs(5);
/// 水温低通滤波的时间常数。设备只报整数 °C，在两个值之间来回跳会让风扇目标抖动；
/// 冷却液本身的时间常数是分钟量级，20 秒的滤波不影响响应。
const COOLANT_TAU: Duration = Duration::from_secs(20);
/// 主板 PWM 采样多久没更新就当作没有信号。
const MB_PWM_STALE: Duration = Duration::from_secs(5);
/// 加载 NVML 失败（没有 N 卡）后隔多久再试。
const GPU_RETRY: Duration = Duration::from_secs(30);

struct Shared {
    status: Status,
    config: Config,
    lcd: lcd::LcdState,
    /// 界面排队的设备操作，控制线程每轮开始时取走执行，结果从通道传回。
    ops: Vec<(DeviceOp, mpsc::Sender<Result<(), String>>)>,
}

enum DeviceOp {
    Bind([u8; 6]),
    Unbind([u8; 6]),
    MbLightSync([u8; 6], bool),
}

/// 设备操作要等控制线程执行；绑定最多 5 秒收敛再加保存，给足余量。
const OP_TIMEOUT: Duration = Duration::from_secs(15);

pub fn serve(devices_enabled: bool) -> Result<()> {
    let config = Config::load_or_create()?;
    log::line(&format!(
        "后台服务启动{}；配置 {}",
        if devices_enabled { "" } else { "（不打开设备）" },
        Config::path().display()
    ));
    let shared = Arc::new(Mutex::new(Shared {
        status: Status::default(),
        config,
        lcd: lcd::LcdState::default(),
        ops: Vec::new(),
    }));

    {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || {
            if let Err(e) = ipc::serve(move |req| handle(&shared, req)) {
                log::line(&format!("命名管道服务退出：{e:#}"));
                std::process::exit(1);
            }
        });
    }

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        // 没有控制台（release 版双击启动）时注册会失败，不影响运行。
        let _ = ctrlc::set_handler(move || stop.store(true, Ordering::SeqCst));
    }

    let h2 = Arc::new(H2Slot::default());
    let control = {
        let (shared, stop, h2) = (Arc::clone(&shared), Arc::clone(&stop), Arc::clone(&h2));
        std::thread::spawn(move || control_loop(&shared, &stop, devices_enabled, h2))
    };
    let screen = {
        let (shared, stop, h2) = (Arc::clone(&shared), Arc::clone(&stop), Arc::clone(&h2));
        std::thread::spawn(move || {
            lcd::run(
                &h2,
                &stop,
                || {
                    let s = shared.lock().unwrap();
                    (s.config.lcd.clone(), s.status.clone())
                },
                |state| {
                    let mut s = shared.lock().unwrap();
                    s.lcd = state;
                },
            )
        })
    };
    tray::run(&TrayHost(Arc::clone(&shared)), &stop);
    stop.store(true, Ordering::SeqCst);
    let _ = control.join();
    let _ = screen.join();
    log::line("后台服务已停止，设备保持最后收到的转速");
    Ok(())
}

fn control_loop(shared: &Mutex<Shared>, stop: &AtomicBool, devices_enabled: bool, h2: Arc<H2Slot>) {
    let mut ctl = Controller::new(devices_enabled, h2);
    let mut last_error: Option<String> = None;
    while !stop.load(Ordering::SeqCst) {
        let tick = Instant::now();
        let (config, ops) = {
            let mut s = shared.lock().unwrap();
            (s.config.clone(), std::mem::take(&mut s.ops))
        };
        ctl.run_ops(ops);
        let status = ctl.tick(&config);
        // 同样的错误每秒都会出现，只在内容变化时记一次。
        if status.last_error != last_error {
            match &status.last_error {
                Some(e) => log::line(e),
                None if last_error.is_some() => log::line("错误已恢复"),
                None => {}
            }
            last_error = status.last_error.clone();
        }
        shared.lock().unwrap().status = status;
        sleep(TICK.saturating_sub(tick.elapsed()));
    }
}

fn apply_config(shared: &Mutex<Shared>, mut config: Config) -> Result<()> {
    config.normalize();
    config.save()?;
    shared.lock().unwrap().config = config;
    Ok(())
}

/// 把设备操作交给控制线程，等它执行完。
fn queue_op(shared: &Mutex<Shared>, address: &str, make: impl FnOnce([u8; 6]) -> DeviceOp) -> Result<()> {
    let mac = lcl_core::parse_mac(address).with_context(|| format!("无效的设备地址：{address}"))?;
    let (tx, rx) = mpsc::channel();
    shared.lock().unwrap().ops.push((make(mac), tx));
    match rx.recv_timeout(OP_TIMEOUT) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(anyhow::anyhow!(e)),
        Err(_) => Err(anyhow::anyhow!("操作超时（后台服务忙或设备没有响应）")),
    }
}

fn handle(shared: &Mutex<Shared>, req: Request) -> Response {
    let result = match req {
        Request::GetStatus => {
            return Response::Status {
                status: shared.lock().unwrap().status.clone(),
            }
        }
        Request::GetConfig => {
            return Response::Config {
                config: shared.lock().unwrap().config.clone(),
            }
        }
        Request::GetAutostart => {
            return Response::Autostart {
                enabled: autostart::is_enabled(),
            }
        }
        Request::GetLightingCaps => {
            let fan_count = shared.lock().unwrap().status.fan_groups.first().map_or(3, |g| g.rpm.len()) as u8;
            return Response::LightingCaps {
                caps: LightingCaps {
                    fan_count,
                    fans: lcl_effects::tl::parameters::for_fans(fan_count),
                    pump: lcl_effects::h2::parameters(),
                },
            };
        }
        Request::GetLcdState => {
            let s = shared.lock().unwrap();
            return Response::LcdState {
                state: lcl_core::ipc::LcdState {
                    description: s.lcd.description.clone(),
                    error: s.lcd.error.clone(),
                    preview_jpeg_base64: s.lcd.preview.as_deref().map(lcl_core::base64),
                },
            };
        }
        Request::Bind { address } => queue_op(shared, &address, DeviceOp::Bind),
        Request::Unbind { address } => queue_op(shared, &address, DeviceOp::Unbind),
        Request::SetMbLightSync { address, enabled } => queue_op(shared, &address, |mac| DeviceOp::MbLightSync(mac, enabled)),
        Request::SetConfig { config } => apply_config(shared, config),
        Request::SetAutostart { enabled } => autostart::set(enabled),
    };
    match result {
        Ok(()) => Response::Ok,
        Err(e) => Response::Error {
            message: format!("{e:#}"),
        },
    }
}

struct TrayHost(Arc<Mutex<Shared>>);

impl tray::Host for TrayHost {
    fn snapshot(&self) -> (Status, Config) {
        let s = self.0.lock().unwrap();
        (s.status.clone(), s.config.clone())
    }

    fn set_config(&self, config: Config) {
        if let Err(e) = apply_config(&self.0, config) {
            log::line(&format!("托盘切换预设失败：{e:#}"));
        }
    }
}

struct Controller {
    started: Instant,
    devices_enabled: bool,
    h2: Arc<H2Slot>,
    wl: Option<Wireless>,
    last_open: Option<Instant>,
    h2_params: Option<H2Params>,
    last_h2_read: Option<Instant>,
    last_h2_sync: Option<Instant>,
    /// 经过限速后实际下发的风扇占空比（%）。
    fan_percent: Option<f32>,
    cpu: CpuUsage,
    cpu_usage: Option<f32>,
    gpu: Option<Gpu>,
    gpu_open_tried: Option<Instant>,
    coolant_filtered: Option<f32>,
    lighting: LightingCtl,
    /// 锁定过滤之前的这一轮无线快照，给状态里的“全部设备”列表用。
    last_full: Option<Snapshot>,
    /// 最近一次有效的主板 PWM 采样（RX 偶尔回坏包时用缓存，不带这个值）。
    mb_pwm: Option<(u8, Instant)>,
    errors: Vec<String>,
}

impl Controller {
    fn new(devices_enabled: bool, h2: Arc<H2Slot>) -> Self {
        Self {
            started: Instant::now(),
            devices_enabled,
            h2,
            wl: None,
            last_open: None,
            h2_params: None,
            last_h2_read: None,
            last_h2_sync: None,
            fan_percent: None,
            cpu: CpuUsage::default(),
            cpu_usage: None,
            gpu: None,
            gpu_open_tried: None,
            coolant_filtered: None,
            lighting: LightingCtl::default(),
            last_full: None,
            mb_pwm: None,
            errors: Vec::new(),
        }
    }

    /// 只有用得到 GPU 读数时（风扇曲线按 GPU 温度、GPU 前馈、屏幕仪表盘）才加载 NVML。
    /// nvml.dll 加载后卸不掉、要占约 20 MB，所以默认的水温控制下后台服务不碰它；
    /// 界面显示的 GPU 读数由界面进程自己读。
    fn update_gpu(&mut self, config: &Config) {
        let c = &config.cooling;
        let needed = (c.mode == CoolingMode::Curve && (c.fan_source == TempSource::Gpu || c.gpu_boost_percent > 0.0))
            || config.lcd.mode == lcl_core::LcdMode::Dashboard;
        // 没有 N 卡时别每秒都去试。
        if needed && self.gpu.is_none() && self.gpu_open_tried.map_or(true, |t| t.elapsed() >= GPU_RETRY) {
            self.gpu_open_tried = Some(Instant::now());
            self.gpu = Gpu::open();
            log::line(if self.gpu.is_some() { "已加载 NVML" } else { "没有可用的 NVIDIA GPU" });
        }
    }

    fn tick(&mut self, config: &Config) -> Status {
        self.update_gpu(config);
        self.errors.clear();
        self.cpu_usage = self.cpu.sample();
        self.ensure_devices();

        let full = self.poll_wireless();
        self.last_full = full.clone();
        // 锁定绑定：本机设备里只保留锁定列表中的，其余一律不碰（槽位也按保留下来的重新编号，和官方一致）。
        let snap = full.map(|mut s| {
            if let (Some(wl), false) = (&self.wl, config.devices.locked.is_empty()) {
                s.devices.retain(|d| d.master != wl.master || config.devices.locked.contains(&d.mac_str()));
            }
            s
        });
        let aio = match (&self.wl, &snap) {
            (Some(wl), Some(snap)) => wl.own_aio(snap).map(|(slot, d)| (slot, d.clone())),
            _ => None,
        };
        self.read_h2();

        // 水温：优先用冷头 USB 读数，没有就用无线记录里的。
        let coolant_raw = self
            .h2_params
            .as_ref()
            .map(|p| f32::from(p.coolant_c))
            .filter(|&c| c > 0.0)
            .or_else(|| aio.as_ref().and_then(|(_, d)| d.coolant_c()).map(f32::from));
        self.coolant_filtered = match (coolant_raw, self.coolant_filtered) {
            (Some(raw), Some(prev)) => Some(low_pass(prev, raw, TICK, COOLANT_TAU)),
            (raw, _) => raw,
        };
        let coolant = self.coolant_filtered;

        let gpu = self.gpu.as_ref().map(Gpu::sample).unwrap_or_default();
        // 没接信号线时 RX 有时报 0% 而不是“无信号”；真实的主板 PWM 不会是 0，按无信号处理。
        if let Some(d) = snap.as_ref().and_then(|s| s.mb_pwm).filter(|&d| d > 0) {
            self.mb_pwm = Some((d, Instant::now()));
        }
        let mb_percent = self
            .mb_pwm
            .filter(|(_, at)| at.elapsed() < MB_PWM_STALE)
            .map(|(d, _)| f32::from(d) * 100.0 / 255.0);

        let square = self.h2.lock().as_ref().map_or(true, |h| h.square);
        let max_pump = if square { 3200.0 } else { 2500.0 };
        let c = &config.cooling;
        let fan_temp = match c.fan_source {
            TempSource::Coolant => coolant,
            TempSource::Gpu => gpu.temp_c,
        };
        let fan_curve_target = match (c.mode, fan_temp) {
            (CoolingMode::Curve, Some(t)) => c.fan_curve.eval(t),
            (CoolingMode::Curve, None) => {
                self.errors.push(format!("没有{}读数，风扇暂用固定值", temp_source_name(c.fan_source)));
                None
            }
            (CoolingMode::Motherboard, _) => {
                if mb_percent.is_none() {
                    self.errors.push("没有采样到主板 PWM（RX 没接信号线？），风扇暂用固定值".into());
                }
                mb_percent
            }
            (CoolingMode::Fixed, _) => None,
        };
        let pump_target = match (c.mode, coolant) {
            (CoolingMode::Curve, Some(t)) => c.pump_curve.eval(t),
            (CoolingMode::Curve, None) => {
                self.errors.push("没有水温读数，泵暂用固定值".into());
                None
            }
            _ => None,
        };
        // 前馈只叠加在曲线输出上；固定和跟随主板模式不加。
        let boost = if c.mode == CoolingMode::Curve {
            boost(self.cpu_usage.unwrap_or(0.0), c.cpu_boost_percent) + boost(gpu.usage.unwrap_or(0.0), c.gpu_boost_percent)
        } else {
            0.0
        };
        let fan_target = (fan_curve_target.unwrap_or(c.fixed_fan_percent) + boost).clamp(MIN_FAN_PERCENT, 100.0);
        let pump_rpm = pump_target.unwrap_or(c.fixed_pump_rpm).clamp(PUMP_MIN_RPM, max_pump).round() as u16;

        let step = c.fan_slew_percent_per_s * TICK.as_secs_f32();
        let fan = match self.fan_percent {
            Some(prev) => prev + (fan_target - prev).clamp(-step, step),
            None => fan_target,
        };
        self.fan_percent = Some(fan);

        if let (Some(wl), Some(snap)) = (self.wl.as_mut(), snap.as_ref()) {
            let out = Outputs { fan, pump_rpm, square, overrides: &c.fan_overrides };
            match drive_wireless(wl, snap, aio.as_ref(), &out) {
                Ok(()) => self.lighting.drive_fans(&config.lighting, wl, snap, &mut self.errors),
                Err(e) => {
                    self.errors.push(format!("无线：{e:#}"));
                    self.wl = None;
                }
            }
        }
        if aio.is_none() {
            let mut slot = self.h2.lock();
            if let Some(h2) = slot.as_mut() {
                match self.lighting.drive_pump(&config.lighting, h2, &mut self.errors) {
                    // 推送灯圈后冷头重新初始化过：马上重发泵速，屏幕也要重推。
                    Ok(true) => {
                        self.last_h2_sync = None;
                        self.h2.bump();
                    }
                    Ok(false) => {}
                    Err(e) => {
                        self.errors.push(format!("冷头灯效推送：{e:#}"));
                        *slot = None;
                        self.h2_params = None;
                    }
                }
            }
            drop(slot);
            self.sync_h2(pump_rpm, percent_to_duty(fan));
        }

        let mut status = self.status(config, coolant, pump_rpm, fan, snap.as_ref(), aio.is_some());
        status.coolant_raw_c = coolant_raw;
        status.gpu_temp_c = gpu.temp_c;
        status.gpu_usage = gpu.usage;
        status.gpu_power_w = gpu.power_w;
        status.mb_pwm_percent = mb_percent;
        status.boost_percent = Some(boost);
        status
    }

    fn ensure_devices(&mut self) {
        if !self.devices_enabled {
            self.errors.push("设备已禁用（--no-devices）".into());
            return;
        }
        let h2_open = self.h2.lock().is_some();
        if h2_open && self.wl.is_some() {
            return;
        }
        if self.last_open.is_some_and(|t| t.elapsed() < REOPEN_INTERVAL) {
            return;
        }
        self.last_open = Some(Instant::now());
        if !h2_open {
            match H2::open() {
                Ok(Some(h2)) => {
                    *self.h2.lock() = Some(h2);
                    self.h2.bump();
                }
                Ok(None) => {}
                Err(e) => self.errors.push(format!("冷头：{e:#}")),
            }
        }
        if self.wl.is_none() {
            match Wireless::connect(false) {
                Ok(wl) => self.wl = wl,
                Err(e) => self.errors.push(format!("无线：{e:#}")),
            }
        }
    }

    fn poll_wireless(&mut self) -> Option<Snapshot> {
        let wl = self.wl.as_mut()?;
        // 偶尔会读到不完整的回复，跳过这一轮、用缓存，不中断保活。
        Some(wl.poll().unwrap_or_else(|_| wl.cached()))
    }

    fn read_h2(&mut self) {
        let mut slot = self.h2.lock();
        let Some(h2) = slot.as_mut() else { return };
        if self.last_h2_read.is_some_and(|t| t.elapsed() < H2_READ_INTERVAL) {
            return;
        }
        self.last_h2_read = Some(Instant::now());
        match h2.get_params() {
            Ok(p) => self.h2_params = Some(p),
            Err(e) => {
                self.errors.push(format!("冷头读取：{e:#}"));
                self.h2_params = None;
                *slot = None;
            }
        }
    }

    fn sync_h2(&mut self, pump_rpm: u16, fan_duty: u8) {
        let mut slot = self.h2.lock();
        let Some(h2) = slot.as_mut() else { return };
        if self.last_h2_sync.is_some_and(|t| t.elapsed() < H2_SYNC_INTERVAL) {
            return;
        }
        self.last_h2_sync = Some(Instant::now());
        if let Err(e) = h2.sync_pump_fan(pump_rpm, [fan_duty; 3]) {
            self.errors.push(format!("冷头设定：{e:#}"));
            *slot = None;
        }
    }

    fn status(
        &self,
        config: &Config,
        coolant: Option<f32>,
        pump_target: u16,
        fan: f32,
        snap: Option<&Snapshot>,
        h2_wireless: bool,
    ) -> Status {
        let mut devices = Vec::new();
        let mut fan_groups = Vec::new();
        let mut pump_rpm = self.h2_params.as_ref().map(|p| p.pump_rpm);

        if let Some(h2) = self.h2.lock().as_ref() {
            devices.push(DeviceInfo {
                name: format!("隐流二代 {}", if h2.square { "LCD-S" } else { "LCD-C" }),
                connection: if h2_wireless { "USB + 无线" } else { "USB" }.into(),
                address: self
                    .h2_params
                    .as_ref()
                    .and_then(|p| p.mac)
                    .map(|m| fmt_mac(&m))
                    .unwrap_or_default(),
            });
        }
        if let (Some(wl), Some(snap)) = (&self.wl, snap) {
            devices.push(DeviceInfo {
                name: "L-Wireless 收发器".into(),
                connection: format!("USB · 频道 {}", wl.channel),
                address: fmt_mac(&wl.master),
            });
            for (_, d) in wl.own_devices(snap) {
                if d.is_aio() {
                    pump_rpm = pump_rpm.or(d.pump_rpm());
                    continue;
                }
                let n = usize::from(d.fan_count);
                fan_groups.push(FanGroup {
                    address: d.mac_str(),
                    kind: d.kind().into(),
                    rpm: d.rpm[..n].to_vec(),
                    pwm: d.pwm[..n].to_vec(),
                });
                devices.push(DeviceInfo {
                    name: format!("{} × {}", d.kind(), d.fan_count),
                    connection: "无线 · 本机绑定".into(),
                    address: d.mac_str(),
                });
            }
        }

        // RX 听到的全部无线设备（锁定过滤之前），给“设备与绑定”页。
        let mut wireless = Vec::new();
        if let (Some(wl), Some(full)) = (&self.wl, &self.last_full) {
            for d in &full.devices {
                let owner = if d.master == wl.master {
                    "ours"
                } else if d.master == [0; 6] {
                    "unbound"
                } else {
                    "other"
                };
                wireless.push(WirelessDevice {
                    address: d.mac_str(),
                    kind: d.kind().into(),
                    owner: owner.into(),
                    master: fmt_mac(&d.master),
                    fan_count: d.fan_count,
                    rx: d.rx_type,
                    channel: d.channel,
                    mb_light_sync: d.mb_light_sync,
                    excluded: owner == "ours" && !config.devices.locked.is_empty() && !config.devices.locked.contains(&d.mac_str()),
                });
            }
        }

        Status {
            uptime_s: self.started.elapsed().as_secs(),
            mode: Some(config.cooling.mode),
            coolant_c: coolant,
            coolant_raw_c: None,
            cpu_usage: self.cpu_usage,
            pump_rpm,
            pump_target_rpm: Some(pump_target),
            fan_percent: Some(fan),
            fan_groups,
            devices,
            wireless,
            transmitter: self.wl.as_ref().map(|wl| (fmt_mac(&wl.master), wl.channel)),
            last_error: (!self.errors.is_empty()).then(|| self.errors.join("；")),
            ..Status::default()
        }
    }

    /// 执行界面排队的设备操作（绑定、解绑、主板灯光同步）。在控制线程里跑，因为无线句柄在这里。
    fn run_ops(&mut self, ops: Vec<(DeviceOp, mpsc::Sender<Result<(), String>>)>) {
        for (op, reply) in ops {
            let result = match self.wl.as_mut() {
                None => Err("无线收发器没有连接".to_string()),
                Some(wl) => {
                    let r = match op {
                        DeviceOp::Bind(mac) => wl.bind(&mac).map(|rx| log::line(&format!("已绑定 {}，rx {rx}", fmt_mac(&mac)))),
                        DeviceOp::Unbind(mac) => wl.unbind(&mac).map(|()| log::line(&format!("已解绑 {}", fmt_mac(&mac)))),
                        DeviceOp::MbLightSync(mac, on) => {
                            let snap = wl.cached();
                            match wl.own_devices(&snap).into_iter().find(|(_, d)| d.mac == mac).map(|(s, d)| (s, d.clone())) {
                                Some((slot, dev)) => wl.set_mb_light_sync(&dev, slot, on),
                                None => Err(anyhow::anyhow!("这个设备没有绑定在本机")),
                            }
                        }
                    };
                    r.map_err(|e| format!("{e:#}"))
                }
            };
            if let Err(e) = &result {
                log::line(&format!("设备操作失败：{e}"));
            }
            let _ = reply.send(result);
        }
    }
}

/// 一阶低通：每步向新值靠近 dt / (tau + dt)。
fn low_pass(prev: f32, raw: f32, dt: Duration, tau: Duration) -> f32 {
    let a = dt.as_secs_f32() / (tau.as_secs_f32() + dt.as_secs_f32());
    prev + a * (raw - prev)
}

fn percent_to_duty(percent: f32) -> u8 {
    (percent.clamp(0.0, 100.0) * 255.0 / 100.0).round() as u8
}

fn temp_source_name(source: TempSource) -> &'static str {
    match source {
        TempSource::Coolant => "水温",
        TempSource::Gpu => "GPU 温度",
    }
}

/// 本轮要下发的输出；风扇占空比是全局值，单风扇覆盖在下发时套用。
struct Outputs<'a> {
    fan: f32,
    pump_rpm: u16,
    square: bool,
    overrides: &'a [FanOverride],
}

impl Outputs<'_> {
    /// 某组第 `index` 个风扇的占空比：有固定值用固定值，否则全局值加偏移。
    fn fan_duty(&self, address: &str, index: usize) -> u8 {
        let percent = match self.overrides.iter().find(|o| o.address == address && usize::from(o.index) == index) {
            Some(FanOverride { fixed_percent: Some(f), .. }) => *f,
            Some(o) => self.fan + o.offset_percent,
            None => self.fan,
        };
        percent_to_duty(percent.clamp(MIN_FAN_PERCENT, 100.0))
    }
}

fn drive_wireless(
    wl: &mut Wireless,
    snap: &Snapshot,
    aio: Option<&(u8, lcl_devices::wireless::RfDevice)>,
    out: &Outputs,
) -> Result<()> {
    wl.send_clock()?;
    if let Some((slot, dev)) = aio {
        // 无线模式下泵速只由参数块里的泵定时器决定。PWM 帧第 4 路对冷头也是泵：
        // 再往那里发占空比会和定时器互相打架（实测泵速在两个值之间来回跳）。
        // 所以和官方一样，不带无线风扇的冷头不发 PWM 帧；带风扇的（CL 版）只改三路风扇，泵那一路回填设备当前值。
        wl.set_aio_params(dev, &aio_params(out.pump_rpm, out.square))?;
        if dev.fan_count > 0 {
            let a = dev.mac_str();
            wl.set_pwm(dev, *slot, [out.fan_duty(&a, 0), out.fan_duty(&a, 1), out.fan_duty(&a, 2), dev.pwm[3]])?;
        }
    }
    let targets: Vec<_> = wl
        .own_devices(snap)
        .into_iter()
        .filter(|(_, d)| !d.is_aio())
        .map(|(slot, d)| (slot, d.clone()))
        .collect();
    for (slot, dev) in targets {
        let a = dev.mac_str();
        let pwm = [0, 1, 2, 3].map(|i| out.fan_duty(&a, i));
        wl.set_pwm(&dev, slot, pwm)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_pass_smooths_integer_flicker() {
        // 26/27 来回跳，滤波后应稳定在中间、波动远小于 1 °C。
        let mut v = 26.0;
        let mut seen = Vec::new();
        for i in 0..200 {
            v = low_pass(v, if i % 2 == 0 { 27.0 } else { 26.0 }, TICK, COOLANT_TAU);
            if i > 150 {
                seen.push(v);
            }
        }
        let (lo, hi) = seen.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(hi - lo < 0.1, "波动 {}", hi - lo);
        assert!((26.3..26.7).contains(&lo));
    }

    #[test]
    fn overrides_apply_per_fan() {
        let overrides = vec![
            FanOverride { address: "aa".into(), index: 0, offset_percent: 10.0, fixed_percent: None },
            FanOverride { address: "aa".into(), index: 2, offset_percent: -50.0, fixed_percent: None },
            FanOverride { address: "aa".into(), index: 1, offset_percent: 0.0, fixed_percent: Some(80.0) },
        ];
        let out = Outputs { fan: 40.0, pump_rpm: 2000, square: true, overrides: &overrides };
        assert_eq!(out.fan_duty("aa", 0), percent_to_duty(50.0));
        assert_eq!(out.fan_duty("aa", 1), percent_to_duty(80.0));
        // 偏移后低于下限时抬到 20%。
        assert_eq!(out.fan_duty("aa", 2), percent_to_duty(MIN_FAN_PERCENT));
        assert_eq!(out.fan_duty("bb", 0), percent_to_duty(40.0));
    }

    #[test]
    fn boost_is_linear_above_ten_percent() {
        assert_eq!(boost(5.0, 20.0), 0.0);
        assert_eq!(boost(55.0, 20.0), 10.0);
        assert_eq!(boost(100.0, 20.0), 20.0);
    }
}
