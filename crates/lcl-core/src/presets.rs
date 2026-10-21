//! 预设档位：风扇和泵的预置曲线（横轴水温）。前端 app/src/curve.ts 里有同样的一份，改动时两边一起改。

use crate::config::{Config, CoolingMode, TempSource};

pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub fan: &'static [[f32; 2]],
    pub pump: &'static [[f32; 2]],
}

pub const PRESETS: [Preset; 3] = [
    Preset {
        id: "quiet",
        name: "静音",
        fan: &[[25.0, 25.0], [32.0, 35.0], [38.0, 55.0], [44.0, 80.0], [48.0, 100.0]],
        pump: &[[25.0, 1900.0], [40.0, 2400.0], [48.0, 3000.0]],
    },
    Preset {
        id: "balanced",
        name: "均衡",
        fan: &[[25.0, 30.0], [30.0, 45.0], [35.0, 65.0], [40.0, 85.0], [45.0, 100.0]],
        pump: &[[25.0, 2200.0], [35.0, 2600.0], [45.0, 3000.0]],
    },
    Preset {
        id: "performance",
        name: "性能",
        fan: &[[25.0, 45.0], [30.0, 60.0], [35.0, 80.0], [40.0, 100.0]],
        pump: &[[25.0, 2800.0], [35.0, 3200.0]],
    },
];

impl Preset {
    /// 套用到配置：切到曲线模式、风扇按水温，其余设置（前馈、单风扇覆盖等）保留。
    pub fn apply(&self, config: &mut Config) {
        let c = &mut config.cooling;
        c.mode = CoolingMode::Curve;
        c.fan_source = TempSource::Coolant;
        c.fan_curve.points = self.fan.to_vec();
        c.pump_curve.points = self.pump.to_vec();
    }

    pub fn matches(&self, config: &Config) -> bool {
        let c = &config.cooling;
        c.mode == CoolingMode::Curve
            && c.fan_source == TempSource::Coolant
            && c.fan_curve.points == self.fan
            && c.pump_curve.points == self.pump
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_the_balanced_preset() {
        assert!(PRESETS[1].matches(&Config::default()));
    }
}
