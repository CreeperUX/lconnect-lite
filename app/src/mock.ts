import type { Api, Config, Status, WirelessDevice } from "./api";
import { evalCurve, PRESETS } from "./curve";

/** 浏览器里开发界面用的假后台：水温缓慢起伏，风扇和泵按配置跟随。 */
export function mockApi(): Api {
  const balanced = PRESETS[1];
  let config: Config = {
    cooling: {
      mode: "curve",
      fixed_fan_percent: 50,
      fixed_pump_rpm: 2400,
      fan_curve: { points: structuredClone(balanced.fan) },
      fan_source: "coolant",
      pump_curve: { points: structuredClone(balanced.pump) },
      fan_slew_percent_per_s: 5,
      cpu_boost_percent: 0,
      gpu_boost_percent: 0,
      fan_overrides: [],
    },
    lighting: {
      enabled: false,
      fans: [{ effect: { mode: "RainbowMorph", colors: [[255, 255, 255]], speed: 2, brightness: 4, direction: "Clockwise", scope: "All", disabled: false }, flip: false }],
      pump: { mode: "RainbowMorph", colors: [[255, 255, 255]], speed: 2, brightness: 4, direction: "Clockwise", scope: "All", disabled: false },
    },
    lcd: { mode: "dashboard", brightness: 80, rotation: 0, media_path: "", max_fps: 20, offline_clock: false },
    devices: { locked: [] },
  };
  const wireless: WirelessDevice[] = [
    { address: "ce:44:ef:e5:66:e1", kind: "TL 无线", owner: "ours", master: "6c:ca:75:e5:66:e4", fan_count: 3, rx: 3, channel: 8, mb_light_sync: false, excluded: false },
    { address: "60:99:d8:e5:66:e1", kind: "隐流二代 方屏", owner: "unbound", master: "00:00:00:00:00:00", fan_count: 0, rx: 254, channel: 8, mb_light_sync: false, excluded: false },
  ];
  // 与 lcl-effects tl::parameters / h2::parameters 的规则一致（只用于浏览器预览）。
  const tlModes = ["Off", "Rainbow", "RainbowMorph", "Static", "Breathing", "Runway", "Meteor", "ColorCycle", "Staggered", "Tide", "Mixing", "Voice", "Door", "Render", "Ripple", "Reflect", "TailChasing", "Paint", "PingPong", "Stack", "CoverCycle", "Wave", "Racing", "Lottery", "Intertwine", "MeteorShower", "Collide", "ElectricCurrent", "Kaleidoscope", "Twinkle"];
  const dirModes = ["Rainbow", "Meteor", "ColorCycle", "Render", "TailChasing", "Stack", "CoverCycle", "Lottery", "MeteorShower", "Wave", "Paint", "Racing"];
  const tlColors = (m: string) =>
    ["Off", "Rainbow", "RainbowMorph", "Voice", "Kaleidoscope", "Twinkle"].includes(m) ? 0
    : ["Static", "Breathing"].includes(m) ? 3
    : ["Runway", "Staggered", "Tide", "Mixing", "TailChasing", "Racing", "Lottery", "Intertwine"].includes(m) ? 2
    : ["ColorCycle", "CoverCycle"].includes(m) ? 3 : 4;
  const caps = {
    fan_count: 3,
    fans: tlModes.map((mode) => ({
      mode,
      min_colors: ["Static", "Breathing"].includes(mode) ? 3 : tlColors(mode) === 0 ? 0 : 1,
      max_colors: tlColors(mode),
      per_fan_colors: ["Static", "Breathing"].includes(mode),
      directions: dirModes.includes(mode) ? (["Clockwise", "CounterClockwise"] as const).slice() : [],
      supports_speed: !["Off", "Static"].includes(mode),
    })),
    pump: ["Off", "Rainbow", "RainbowMorph", "Static", "Breathing", "Runway", "Meteor", "TaiChi", "Twinkle", "Voice", "Pump", "Bounce"].map((mode) => {
      const max = ["Static", "Breathing", "Voice", "Pump"].includes(mode) ? 1 : ["Runway", "Meteor", "TaiChi", "Bounce"].includes(mode) ? 4 : 0;
      return {
        mode,
        min_colors: Math.min(max, 1),
        max_colors: max,
        per_fan_colors: false,
        directions: ["Rainbow", "Meteor", "TaiChi", "Voice", "Pump"].includes(mode) ? (["Clockwise", "CounterClockwise"] as const).slice() : [],
        supports_speed: !["Off", "Static"].includes(mode),
      };
    }),
  };
  let autostart = false;
  const started = Date.now();
  let fan = 50;
  let pump = 2070;
  const boost = (usage: number, k: number) => Math.min(Math.max((usage - 10) / 90, 0), 1) * k;

  return {
    async getStatus(): Promise<Status> {
      const t = (Date.now() - started) / 1000;
      const coolant = 29 + 3 * Math.sin(t / 30);
      const cpu = 18 + 14 * Math.max(0, Math.sin(t / 7)) + 4 * Math.random();
      const gpuUsage = 30 + 25 * Math.max(0, Math.sin(t / 11));
      const gpuTemp = 38 + gpuUsage * 0.3;
      const c = config.cooling;
      const source = c.fan_source === "gpu" ? gpuTemp : coolant;
      const add = c.mode === "curve" ? boost(cpu, c.cpu_boost_percent) + boost(gpuUsage, c.gpu_boost_percent) : 0;
      const base = c.mode === "curve" ? evalCurve(c.fan_curve.points, source) ?? c.fixed_fan_percent : c.fixed_fan_percent;
      const fanTarget = Math.min(100, Math.max(20, base + add));
      const pumpTarget = Math.round(c.mode === "curve" ? evalCurve(c.pump_curve.points, coolant) ?? c.fixed_pump_rpm : c.fixed_pump_rpm);
      fan += Math.max(-c.fan_slew_percent_per_s, Math.min(c.fan_slew_percent_per_s, fanTarget - fan));
      pump += (pumpTarget + 80 - pump) * 0.15;
      const address = "ce:44:ef:e5:66:e1";
      const perFan = [0, 1, 2].map((i) => {
        const o = c.fan_overrides.find((x) => x.address === address && x.index === i);
        const p = o?.fixed_percent ?? fan + (o?.offset_percent ?? 0);
        return Math.min(100, Math.max(20, p));
      });
      return {
        uptime_s: Math.round(t),
        mode: c.mode,
        coolant_c: coolant,
        coolant_raw_c: Math.round(coolant),
        cpu_usage: cpu,
        gpu_temp_c: gpuTemp,
        gpu_usage: gpuUsage,
        gpu_power_w: 80 + gpuUsage * 4,
        mb_pwm_percent: null,
        boost_percent: add,
        pump_rpm: Math.round(pump),
        pump_target_rpm: pumpTarget,
        fan_percent: fan,
        fan_groups: [
          {
            address,
            kind: "TL 无线",
            rpm: perFan.map((p, i) => Math.round(p * 25.2 + i * 2)),
            pwm: perFan.map((p) => Math.round((p * 255) / 100)),
          },
        ],
        devices: [
          { name: "隐流二代 LCD-S", connection: "USB", address: "60:99:d8:e5:66:e1" },
          { name: "L-Wireless 收发器", connection: "USB · 频道 8", address: "6c:ca:75:e5:66:e4" },
          { name: "TL 无线 × 3", connection: "无线 · 本机绑定", address },
        ],
        wireless: wireless.map((w) => ({ ...w, excluded: w.owner === "ours" && config.devices.locked.length > 0 && !config.devices.locked.includes(w.address) })),
        transmitter: ["6c:ca:75:e5:66:e4", 8],
        last_error: c.mode === "motherboard" ? "没有采样到主板 PWM（RX 没接信号线？），风扇暂用固定值" : null,
      };
    },
    async getConfig() {
      return structuredClone(config);
    },
    async setConfig(next) {
      config = structuredClone(next);
    },
    async getLightingCaps() {
      return caps;
    },
    async getLcdState() {
      const descs = { unmanaged: "未控制（保持设备现有画面）", dashboard: "仪表盘", image: "图片：示例.png", gif: "GIF：示例.gif（24 帧）" };
      return { description: descs[config.lcd.mode], error: null, preview_jpeg_base64: null };
    },
    async bind(address) {
      const w = wireless.find((x) => x.address === address)!;
      await new Promise((r) => setTimeout(r, 1200));
      Object.assign(w, { owner: "ours", master: "6c:ca:75:e5:66:e4", rx: 1, channel: 8 });
    },
    async unbind(address) {
      const w = wireless.find((x) => x.address === address)!;
      await new Promise((r) => setTimeout(r, 1200));
      Object.assign(w, { owner: "unbound", master: "00:00:00:00:00:00", rx: 0 });
    },
    async setMbLightSync(address, enabled) {
      wireless.find((x) => x.address === address)!.mb_light_sync = enabled;
    },
    async pickMedia(kind) {
      return kind === "gif" ? "C:\\模拟\\示例.gif" : "C:\\模拟\\示例.png";
    },
    async getAutostart() {
      return autostart;
    },
    async setAutostart(enabled) {
      autostart = enabled;
    },
    async exportConfig() {
      return "C:\\模拟\\lconnect-lite.toml";
    },
    async importConfig() {
      return null;
    },
    async startDaemon() {},
    async paths() {
      return { config: "%APPDATA%\\LConnect Lite\\config.toml", log: "%LOCALAPPDATA%\\LConnect Lite\\daemon.log" };
    },
  };
}
