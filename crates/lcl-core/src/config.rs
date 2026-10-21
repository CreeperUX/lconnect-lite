//! 配置文件：`%APPDATA%\LConnect Lite\config.toml`。

use anyhow::{Context, Result};
use lcl_effects::types::RgbRegionConfig;
use lcl_effects::{RgbEffect, RgbMode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 风扇占空比下限；再低 TL/CL 风扇可能停转或抖动。
pub const MIN_FAN_PERCENT: f32 = 20.0;
pub const PUMP_MIN_RPM: f32 = 1600.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub cooling: Cooling,
    pub lighting: Lighting,
    pub lcd: Lcd,
    pub devices: Devices,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Devices {
    /// 锁定绑定：非空时只控制列表里的无线设备（MAC），其他绑定在本机的设备一律不碰。
    /// 和官方的“锁定”一样只在主机这边生效，不向设备发任何东西。
    pub locked: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LcdMode {
    /// 不碰屏幕，保持设备上现有的画面（比如 L-Connect 之前设的）。
    #[default]
    Unmanaged,
    /// 传感器仪表盘（水温、泵速、风扇、CPU、GPU），每秒刷新。
    Dashboard,
    /// 一张静态图片。
    Image,
    /// GIF 动图，拆成 JPEG 帧循环播放。
    Gif,
}

/// 冷头屏幕（480×480）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lcd {
    pub mode: LcdMode,
    /// 0-100。
    pub brightness: u8,
    /// 0 / 90 / 180 / 270，顺时针。屏幕本身不会旋转，画面在电脑上转好再发。
    pub rotation: u16,
    /// 图片或 GIF 的路径。
    pub media_path: String,
    /// GIF 最高帧率。
    pub max_fps: u8,
    /// 离线时钟：电脑没在推画面（比如关机后冷头仍通电）时屏幕显示时钟。
    pub offline_clock: bool,
}

impl Default for Lcd {
    fn default() -> Self {
        Self {
            mode: LcdMode::Unmanaged,
            brightness: 80,
            rotation: 0,
            media_path: String::new(),
            max_fps: 20,
            offline_clock: false,
        }
    }
}

/// 灯效。设备没有内置效果，由后台服务渲染成逐帧颜色下发。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lighting {
    /// 关闭时完全不碰灯，设备保持现有的灯效（比如 L-Connect 之前设的）。
    pub enabled: bool,
    /// 无线风扇：一个“全部”区域，或者“上弧”“下弧”两个区域。
    pub fans: Vec<RgbRegionConfig>,
    /// 冷头灯圈。
    pub pump: RgbEffect,
}

impl Default for Lighting {
    fn default() -> Self {
        let rainbow = RgbEffect {
            mode: RgbMode::RainbowMorph,
            ..RgbEffect::default()
        };
        Self {
            enabled: false,
            fans: vec![RgbRegionConfig {
                effect: rainbow.clone(),
                flip: false,
            }],
            pump: rainbow,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoolingMode {
    /// 风扇和泵都用固定值。
    Fixed,
    /// 按温度查曲线。
    Curve,
    /// 风扇跟随 RX 采样到的主板 PWM，泵用固定值。需要接主板 PWM 信号线。
    Motherboard,
}

/// 风扇曲线的横轴用哪个温度。泵曲线始终用水温。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TempSource {
    #[default]
    Coolant,
    Gpu,
}

/// 单个风扇的覆盖设置：固定值优先，否则在全局输出上加偏移。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FanOverride {
    /// 风扇组的无线地址，如 "ce:44:ef:e5:66:e1"。
    pub address: String,
    /// 组内第几个风扇，从 0 开始。
    pub index: u8,
    #[serde(default)]
    pub offset_percent: f32,
    #[serde(default)]
    pub fixed_percent: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cooling {
    pub mode: CoolingMode,
    pub fixed_fan_percent: f32,
    pub fixed_pump_rpm: f32,
    /// 温度（°C，来源见 `fan_source`）→ 风扇占空比（%）。
    pub fan_curve: Curve,
    pub fan_source: TempSource,
    /// 水温（°C）→ 泵转速（rpm）。
    pub pump_curve: Curve,
    /// 风扇占空比每秒最多变化多少个百分点，避免转速跳变。
    pub fan_slew_percent_per_s: f32,
    /// 前馈：CPU 满载时在曲线输出上额外加多少个百分点（线性，10% 占用以下不加）。0 关闭。
    pub cpu_boost_percent: f32,
    /// 前馈：GPU 满载时额外加多少个百分点。0 关闭。
    pub gpu_boost_percent: f32,
    pub fan_overrides: Vec<FanOverride>,
}

impl Default for Cooling {
    fn default() -> Self {
        Self {
            mode: CoolingMode::Curve,
            fixed_fan_percent: 50.0,
            fixed_pump_rpm: 2400.0,
            fan_curve: Curve {
                points: vec![[25.0, 30.0], [30.0, 45.0], [35.0, 65.0], [40.0, 85.0], [45.0, 100.0]],
            },
            fan_source: TempSource::Coolant,
            pump_curve: Curve {
                points: vec![[25.0, 2200.0], [35.0, 2600.0], [45.0, 3000.0]],
            },
            fan_slew_percent_per_s: 5.0,
            cpu_boost_percent: 0.0,
            gpu_boost_percent: 0.0,
            fan_overrides: Vec::new(),
        }
    }
}

/// 前馈量：占用率 10% 以下不加，之后线性到满载时的 `boost`。
pub fn boost(usage_percent: f32, boost: f32) -> f32 {
    ((usage_percent - 10.0) / 90.0).clamp(0.0, 1.0) * boost
}

/// 分段线性曲线，点按 x 升序；超出两端时取端点值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Curve {
    pub points: Vec<[f32; 2]>,
}

impl Curve {
    pub fn eval(&self, x: f32) -> Option<f32> {
        let pts = &self.points;
        let (first, last) = (pts.first()?, pts.last()?);
        if x <= first[0] {
            return Some(first[1]);
        }
        if x >= last[0] {
            return Some(last[1]);
        }
        pts.windows(2).find(|w| x <= w[1][0]).map(|w| {
            let [x0, y0] = w[0];
            let [x1, y1] = w[1];
            if x1 == x0 {
                y1
            } else {
                y0 + (y1 - y0) * (x - x0) / (x1 - x0)
            }
        })
    }

    pub fn sort(&mut self) {
        self.points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    }

    /// 散热曲线必须随温度不降：温度越高转速不能越低。每个点至少等于前一个点。
    pub fn make_monotone(&mut self) {
        for i in 1..self.points.len() {
            let prev = self.points[i - 1][1];
            if self.points[i][1] < prev {
                self.points[i][1] = prev;
            }
        }
    }
}

/// 后台服务日志：`%LOCALAPPDATA%\LConnect Lite\daemon.log`。
pub fn log_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(|| PathBuf::from("."), PathBuf::from);
    base.join("LConnect Lite").join("daemon.log")
}

impl Config {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map_or_else(|| PathBuf::from("."), PathBuf::from);
        base.join("LConnect Lite").join("config.toml")
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// 解析并规范化；用于导入别处导出的配置。
    pub fn from_toml(text: &str) -> Result<Self> {
        let mut config: Self = toml::from_str(text).context("不是有效的 LConnect Lite 配置文件")?;
        config.normalize();
        Ok(config)
    }

    /// 读配置；文件不存在时写一份默认配置。
    pub fn load_or_create() -> Result<Self> {
        let path = Self::path();
        if !path.exists() {
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }
        let text = std::fs::read_to_string(&path).with_context(|| format!("读取 {}", path.display()))?;
        let mut config: Self = toml::from_str(&text).with_context(|| format!("解析 {}", path.display()))?;
        config.normalize();
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?).with_context(|| format!("写入 {}", path.display()))
    }

    /// 曲线排序、数值截到硬件允许的范围。
    pub fn normalize(&mut self) {
        let c = &mut self.cooling;
        c.fan_curve.sort();
        c.pump_curve.sort();
        c.fixed_fan_percent = c.fixed_fan_percent.clamp(MIN_FAN_PERCENT, 100.0);
        c.fixed_pump_rpm = c.fixed_pump_rpm.max(PUMP_MIN_RPM);
        c.fan_slew_percent_per_s = c.fan_slew_percent_per_s.clamp(0.5, 100.0);
        for p in &mut c.fan_curve.points {
            p[1] = p[1].clamp(MIN_FAN_PERCENT, 100.0);
        }
        for p in &mut c.pump_curve.points {
            p[1] = p[1].max(PUMP_MIN_RPM);
        }
        c.fan_curve.make_monotone();
        c.pump_curve.make_monotone();
        c.cpu_boost_percent = c.cpu_boost_percent.clamp(0.0, 50.0);
        c.gpu_boost_percent = c.gpu_boost_percent.clamp(0.0, 50.0);
        for o in &mut c.fan_overrides {
            o.offset_percent = o.offset_percent.clamp(-50.0, 50.0);
            o.fixed_percent = o.fixed_percent.map(|f| f.clamp(MIN_FAN_PERCENT, 100.0));
        }
        // 偏移为 0 且没有固定值的覆盖等于没设，去掉。
        c.fan_overrides.retain(|o| o.offset_percent != 0.0 || o.fixed_percent.is_some());

        let l = &mut self.lcd;
        l.brightness = l.brightness.min(100);
        l.rotation = (l.rotation / 90 % 4) * 90;
        l.max_fps = l.max_fps.clamp(1, 30);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_interpolates_and_clamps() {
        let c = Curve { points: vec![[20.0, 40.0], [30.0, 60.0], [40.0, 100.0]] };
        assert_eq!(c.eval(10.0), Some(40.0));
        assert_eq!(c.eval(25.0), Some(50.0));
        assert_eq!(c.eval(35.0), Some(80.0));
        assert_eq!(c.eval(50.0), Some(100.0));
        assert_eq!(Curve::default().eval(30.0), None);
    }

    #[test]
    fn normalize_sorts_and_forbids_falling_curves() {
        let mut config = Config::default();
        config.cooling.fan_curve.points = vec![[50.0, 20.0], [25.0, 30.0], [45.0, 100.0], [35.0, 10.0]];
        config.normalize();
        assert_eq!(
            config.cooling.fan_curve.points,
            vec![[25.0, 30.0], [35.0, 30.0], [45.0, 100.0], [50.0, 100.0]]
        );
    }

    #[test]
    fn default_config_round_trips_through_toml() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }

    #[test]
    fn partial_file_falls_back_to_defaults() {
        let config: Config = toml::from_str("[cooling]\nmode = \"fixed\"\n").unwrap();
        assert_eq!(config.cooling.mode, CoolingMode::Fixed);
        assert_eq!(config.cooling.fixed_fan_percent, 50.0);
    }
}
