//! NVIDIA GPU 温度、占用率、功耗：通过显卡驱动自带的 nvml.dll，不需要额外驱动。
//! 没有 N 卡或驱动时 `Gpu::open()` 返回 None，其余功能照常。
//! 注意：nvml.dll 加载后即使 drop 也不会从进程里卸载，私有内存约 20 MB。

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;

pub struct Gpu {
    nvml: Nvml,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GpuSample {
    pub temp_c: Option<f32>,
    pub usage: Option<f32>,
    pub power_w: Option<f32>,
}

impl Gpu {
    pub fn open() -> Option<Self> {
        let nvml = Nvml::init().ok()?;
        (nvml.device_count().ok()? > 0).then_some(Self { nvml })
    }

    /// 读第 0 块 N 卡；单项读取失败只让那一项为空。
    pub fn sample(&self) -> GpuSample {
        let Ok(dev) = self.nvml.device_by_index(0) else {
            return GpuSample::default();
        };
        GpuSample {
            temp_c: dev.temperature(TemperatureSensor::Gpu).ok().map(|t| t as f32),
            usage: dev.utilization_rates().ok().map(|u| u.gpu as f32),
            power_w: dev.power_usage().ok().map(|mw| mw as f32 / 1000.0),
        }
    }
}
