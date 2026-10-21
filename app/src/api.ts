import { invoke } from "@tauri-apps/api/core";
import { mockApi } from "./mock";

// 与 crates/lcl-core 的 Status / Config 一一对应。
export type Mode = "fixed" | "curve" | "motherboard";
export type TempSource = "coolant" | "gpu";
export type Point = [number, number];

export interface FanGroup { address: string; kind: string; rpm: number[]; pwm: number[] }
export interface WirelessDevice {
  address: string;
  kind: string;
  owner: "ours" | "unbound" | "other";
  master: string;
  fan_count: number;
  rx: number;
  channel: number;
  mb_light_sync: boolean;
  excluded: boolean;
}
export interface DeviceInfo { name: string; connection: string; address: string }

export interface Status {
  uptime_s: number;
  mode: Mode | null;
  /** 低通滤波后的水温，曲线用的就是它。 */
  coolant_c: number | null;
  /** 设备报告的原始水温（整数 °C）。 */
  coolant_raw_c: number | null;
  cpu_usage: number | null;
  gpu_temp_c: number | null;
  gpu_usage: number | null;
  gpu_power_w: number | null;
  mb_pwm_percent: number | null;
  boost_percent: number | null;
  pump_rpm: number | null;
  pump_target_rpm: number | null;
  fan_percent: number | null;
  fan_groups: FanGroup[];
  devices: DeviceInfo[];
  wireless: WirelessDevice[];
  /** [MAC, 频道] */
  transmitter: [string, number] | null;
  last_error: string | null;
}

export interface FanOverride {
  address: string;
  index: number;
  offset_percent: number;
  fixed_percent: number | null;
}

export interface Cooling {
  mode: Mode;
  fixed_fan_percent: number;
  fixed_pump_rpm: number;
  fan_curve: { points: Point[] };
  fan_source: TempSource;
  pump_curve: { points: Point[] };
  fan_slew_percent_per_s: number;
  cpu_boost_percent: number;
  gpu_boost_percent: number;
  fan_overrides: FanOverride[];
}

/** 与 lcl-effects 的 RgbMode 变体名一致。 */
export type RgbMode = string;
export type RgbDirection = "Clockwise" | "CounterClockwise";
export type RgbScope = "All" | "Top" | "Bottom";
export type Rgb = [number, number, number];

export interface RgbEffect {
  mode: RgbMode;
  colors: Rgb[];
  /** 0-4 */
  speed: number;
  /** 0-4 */
  brightness: number;
  direction: RgbDirection;
  scope: RgbScope;
  disabled: boolean;
}

export interface RgbRegion { effect: RgbEffect; flip: boolean }

export interface Lighting {
  enabled: boolean;
  fans: RgbRegion[];
  pump: RgbEffect;
}

export interface EffectParams {
  mode: RgbMode;
  min_colors: number;
  max_colors: number;
  per_fan_colors: boolean;
  directions: RgbDirection[];
  supports_speed: boolean;
}

export interface LightingCaps { fan_count: number; fans: EffectParams[]; pump: EffectParams[] }

export type LcdMode = "unmanaged" | "dashboard" | "image" | "gif";

export interface Lcd {
  mode: LcdMode;
  brightness: number;
  rotation: number;
  media_path: string;
  max_fps: number;
  offline_clock: boolean;
}

export interface LcdState { description: string; error: string | null; preview_jpeg_base64: string | null }

export interface Config { cooling: Cooling; lighting: Lighting; lcd: Lcd; devices: { locked: string[] } }

export interface Api {
  getStatus(): Promise<Status>;
  getConfig(): Promise<Config>;
  setConfig(config: Config): Promise<void>;
  getLightingCaps(): Promise<LightingCaps>;
  getLcdState(): Promise<LcdState>;
  bind(address: string): Promise<void>;
  unbind(address: string): Promise<void>;
  setMbLightSync(address: string, enabled: boolean): Promise<void>;
  /** 弹出打开对话框选图片或 GIF；取消时返回 null。 */
  pickMedia(kind: "image" | "gif"): Promise<string | null>;
  getAutostart(): Promise<boolean>;
  setAutostart(enabled: boolean): Promise<void>;
  /** 弹出保存对话框，把当前配置导出为 TOML；用户取消时返回 null。 */
  exportConfig(): Promise<string | null>;
  /** 弹出打开对话框，读 TOML 并应用；用户取消时返回 null。 */
  importConfig(): Promise<string | null>;
  /** 启动和界面放在同一目录的 lcl-daemon.exe。 */
  startDaemon(): Promise<void>;
  paths(): Promise<{ config: string; log: string }>;
}

const inTauri = "__TAURI_INTERNALS__" in window;

export const isMock = !inTauri && import.meta.env.DEV;

/** 在 Tauri 里走命名管道；开发时直接用浏览器打开则用模拟数据。 */
export const api: Api = isMock
  ? mockApi()
  : {
      getStatus: () => invoke<Status>("get_status"),
      getConfig: () => invoke<Config>("get_config"),
      setConfig: (config) => invoke<void>("set_config", { config }),
      getLightingCaps: () => invoke<LightingCaps>("get_lighting_caps"),
      getLcdState: () => invoke<LcdState>("get_lcd_state"),
      bind: (address) => invoke<void>("bind_device", { address }),
      unbind: (address) => invoke<void>("unbind_device", { address }),
      setMbLightSync: (address, enabled) => invoke<void>("set_mb_light_sync", { address, enabled }),
      pickMedia: (kind) => invoke<string | null>("pick_media", { kind }),
      getAutostart: () => invoke<boolean>("get_autostart"),
      setAutostart: (enabled) => invoke<void>("set_autostart", { enabled }),
      exportConfig: () => invoke<string | null>("export_config"),
      importConfig: () => invoke<string | null>("import_config"),
      startDaemon: () => invoke<void>("start_daemon"),
      paths: () => invoke<{ config: string; log: string }>("paths"),
    };
