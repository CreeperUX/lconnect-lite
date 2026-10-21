import { api, type Lighting, type LightingPreview, type Track } from "./api";

/** 灯效预览缓存：同一份灯效配置只渲染一次（在界面进程里算，几十毫秒）。 */
const cache = new Map<string, Promise<LightingPreview>>();

export function previewFor(lighting: Lighting, fanCount: number): Promise<LightingPreview> {
  const key = JSON.stringify([lighting.fans, lighting.pump, fanCount]);
  let p = cache.get(key);
  if (!p) {
    p = api.previewLighting(lighting, fanCount);
    p.catch(() => cache.delete(key));
    cache.set(key, p);
    if (cache.size > 40) cache.delete(cache.keys().next().value!);
  }
  return p;
}

/**
 * 发给灯珠的字节是线性的 PWM 占空比，屏幕上的颜色值却是 sRGB 编码过的。直接当 sRGB 画会比实物暗、灰，
 * 浅色更明显（#c8f5ff 实物接近白色）。这里按 sRGB 编码换算一次，预览接近灯珠实际发出的光。
 */
const LED_TO_SRGB = Array.from({ length: 256 }, (_, v) => {
  const l = v / 255;
  const e = l <= 0.0031308 ? 12.92 * l : 1.055 * l ** (1 / 2.4) - 0.055;
  return Math.round(e * 255).toString(16).padStart(2, "0");
});

/** 某一时刻某颗灯珠看上去的颜色；全黑（或太暗看不见）返回 null。 */
export function ledAt(track: Track | null | undefined, led: number, tMs: number): string | null {
  if (!track || track.frames.length === 0) return null;
  const frame = track.frames[Math.floor(tMs / Math.max(track.ms, 1)) % track.frames.length];
  const hex = frame.substr(led * 6, 6);
  if (hex.length < 6) return null;
  const r = parseInt(hex.slice(0, 2), 16), g = parseInt(hex.slice(2, 4), 16), b = parseInt(hex.slice(4, 6), 16);
  return Math.max(r, g, b) < 14 ? null : `#${LED_TO_SRGB[r]}${LED_TO_SRGB[g]}${LED_TO_SRGB[b]}`;
}

/** 每帧回调一次，返回停止函数。页面隐藏时浏览器会自动暂停 rAF。 */
export function animate(draw: (tMs: number) => void): () => void {
  let id = 0;
  const start = performance.now();
  const loop = (now: number) => {
    draw(now - start);
    id = requestAnimationFrame(loop);
  };
  id = requestAnimationFrame(loop);
  return () => cancelAnimationFrame(id);
}

/** 风扇组每只 26 颗：前 13 颗上灯带、后 13 颗下灯带。 */
export const LEDS_PER_FAN = 26;
export const PUMP_LEDS = 24;
