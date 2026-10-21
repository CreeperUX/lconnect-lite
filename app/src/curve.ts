import type { Config, Point } from "./api";

/** 分段线性插值，超出两端取端点值（与 lcl-core 的 Curve::eval 一致）。 */
export function evalCurve(points: Point[], x: number): number | null {
  if (points.length === 0) return null;
  if (x <= points[0][0]) return points[0][1];
  const last = points[points.length - 1];
  if (x >= last[0]) return last[1];
  for (let i = 1; i < points.length; i++) {
    const [x0, y0] = points[i - 1];
    const [x1, y1] = points[i];
    if (x <= x1) return x1 === x0 ? y1 : y0 + ((y1 - y0) * (x - x0)) / (x1 - x0);
  }
  return last[1];
}

export interface Preset {
  id: "quiet" | "balanced" | "performance";
  name: string;
  fan: Point[];
  pump: Point[];
}

/** 与 crates/lcl-core/src/presets.rs 一致，改动时两边一起改。 */
export const PRESETS: Preset[] = [
  {
    id: "quiet",
    name: "静音",
    fan: [[25, 25], [32, 35], [38, 55], [44, 80], [48, 100]],
    pump: [[25, 1900], [40, 2400], [48, 3000]],
  },
  {
    id: "balanced",
    name: "均衡",
    fan: [[25, 30], [30, 45], [35, 65], [40, 85], [45, 100]],
    pump: [[25, 2200], [35, 2600], [45, 3000]],
  },
  {
    id: "performance",
    name: "性能",
    fan: [[25, 45], [30, 60], [35, 80], [40, 100]],
    pump: [[25, 2800], [35, 3200]],
  },
];

const samePoints = (a: Point[], b: Point[]) =>
  a.length === b.length && a.every((p, i) => p[0] === b[i][0] && p[1] === b[i][1]);

/** 当前配置正好是某个预设时返回它，否则（自定义或固定模式）返回 null。 */
export function activePreset(config: Config): Preset | null {
  const c = config.cooling;
  if (c.mode !== "curve" || c.fan_source !== "coolant") return null;
  return PRESETS.find((p) => samePoints(p.fan, c.fan_curve.points) && samePoints(p.pump, c.pump_curve.points)) ?? null;
}

export function applyPreset(config: Config, preset: Preset): Config {
  // 与 lcl-core presets::Preset::apply 一致：前馈、单风扇覆盖等保留。
  const next = structuredClone(config);
  next.cooling.mode = "curve";
  next.cooling.fan_source = "coolant";
  next.cooling.fan_curve.points = structuredClone(preset.fan);
  next.cooling.pump_curve.points = structuredClone(preset.pump);
  return next;
}
