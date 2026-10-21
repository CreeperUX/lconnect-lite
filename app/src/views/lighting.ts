import { api, type EffectParams, type Lighting, type LightingCaps, type LightingPreview, type LightingScene, type Rgb, type RgbEffect, type RgbRegion } from "../api";
import { h, s } from "../dom";
import { icon } from "../icons";
import { animate, ledAt, LEDS_PER_FAN, PUMP_LEDS, previewFor } from "../lighting-preview";
import { modeName } from "../modes";
import { conceal, reveal } from "../motion";
import { commit, store } from "../store";
import { colorPicker, confirmDialog, promptDialog, segmented, toast, toggle } from "../ui";
import type { View } from "./view";

type ZoneId = "top" | "bottom" | "pump";
const ZONES: ZoneId[] = ["top", "bottom", "pump"];
const ZN: Record<ZoneId, string> = { top: "上灯带", bottom: "下灯带", pump: "冷头灯框" };
const PALETTE: Rgb[] = [[242, 163, 58], [255, 90, 110], [255, 214, 10], [91, 224, 122], [74, 164, 255], [168, 107, 255], [244, 241, 234]];
const SPEEDS = ["最慢", "慢", "中", "快", "最快"];
const BRIGHTS = ["20%", "40%", "60%", "80%", "100%"];
/** 拖颜色选择器时停手这么久才写入风扇。 */
const SETTLE_MS = 350;

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const toHex = ([r, g, b]: Rgb) => `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
const fromHex = (hex: string): Rgb => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as Rgb;

function effect(mode: string, colors: Rgb[], brightness = 4, speed = 2): RgbEffect {
  return { mode, colors, speed, brightness, direction: "Clockwise", scope: "All", disabled: false };
}
const AMBER: Rgb = [242, 163, 58];
/** 内置场景。夜间用最低一档亮度（0 档在冷头上等于关灯）。 */
const BUILTIN: (LightingScene & { dot: string })[] = [
  { name: "彩虹", dot: "conic-gradient(#ff5a5a, #ffc24a, #5be07a, #4ab8ff, #a86bff, #ff5a5a)", fans: [{ effect: effect("Rainbow", []), flip: false }], pump: effect("Rainbow", []) },
  { name: "琥珀", dot: "#f2a33a", fans: [{ effect: effect("Static", [AMBER, AMBER, AMBER], 3), flip: false }], pump: effect("Static", [AMBER], 3) },
  { name: "夜间", dot: "#7a5320", fans: [{ effect: effect("Breathing", [AMBER, AMBER, AMBER], 1, 0), flip: false }], pump: effect("Static", [AMBER], 1) },
  { name: "关灯", dot: "transparent", fans: [{ effect: effect("Off", []), flip: false }], pump: effect("Off", []) },
];

/** 风扇的上、下灯带各取一个效果（“整体”时两边是同一个）。 */
function fanZones(fans: RgbRegion[]): { top: RgbEffect; bottom: RgbEffect; flip: boolean } {
  const find = (scope: string) => fans.find((r) => r.effect.scope === scope) ?? fans.find((r) => r.effect.scope === "All") ?? fans[0];
  return { top: structuredClone(find("Top").effect), bottom: structuredClone(find("Bottom").effect), flip: fans.some((r) => r.flip) };
}

/** 两条灯带一样就存成一个“整体”区域，不一样就存成上、下两个。 */
function joinFans(top: RgbEffect, bottom: RgbEffect, flip: boolean): RgbRegion[] {
  const strip = (e: RgbEffect) => ({ ...e, scope: "All" as const });
  if (same(strip(top), strip(bottom))) return [{ effect: strip(top), flip }];
  return [
    { effect: { ...top, scope: "Top" }, flip },
    { effect: { ...bottom, scope: "Bottom" }, flip },
  ];
}

export function lighting(): View {
  let caps: LightingCaps | null = null;
  let draft: Lighting | null = store.config ? structuredClone(store.config.lighting) : null;
  let sel: ZoneId[] = ["top", "bottom"];
  let slot = 0;
  let preview: LightingPreview | null = null;
  let previewKey = "";
  let fanTimer = 0;
  let fansPending = false;
  /** 冷头灯框改过但还没发送。 */
  let pumpTouched = false;
  let open = false;
  let active = -1;

  const fanCount = () => Math.min(store.status?.fan_groups[0]?.rpm.length ?? caps?.fan_count ?? 3, 4) || 3;
  const sent = () => store.config?.lighting;
  const pumpDirty = () => draft != null && sent() != null && pumpTouched && !same(draft.pump, sent()!.pump);
  const zoneEffect = (id: ZoneId): RgbEffect | null => {
    if (!draft) return null;
    if (id === "pump") return draft.pump;
    return fanZones(draft.fans)[id];
  };
  const paramsOf = (id: ZoneId, mode: string): EffectParams | undefined => (id === "pump" ? caps?.pump : caps?.fans)?.find((p) => p.mode === mode);
  const colorCount = (id: ZoneId, e: RgbEffect) => {
    const p = paramsOf(id, e.mode);
    if (!p || p.max_colors === 0) return 0;
    return id !== "pump" && p.per_fan_colors ? fanCount() : p.max_colors;
  };
  /** 让颜色数量、方向落在效果允许的范围里。 */
  function fit(id: ZoneId, e: RgbEffect) {
    const p = paramsOf(id, e.mode);
    if (!p) return;
    const want = Math.max(colorCount(id, e), p.min_colors);
    while (e.colors.length < want) e.colors.push(e.colors.at(-1) ?? PALETTE[e.colors.length % PALETTE.length]);
    if (want > 0 && e.colors.length > want) e.colors.length = want;
    if (p.directions.length && !p.directions.includes(e.direction)) e.direction = p.directions[0];
  }

  // ---------- 修改 ----------
  /** 改选中的区域。风扇部分很快（约 1 秒的无线上传），直接写入；冷头灯框只改草稿，等“发送”。 */
  function apply(label: string, mutate: (e: RgbEffect, id: ZoneId) => void, settle = false, bothStrips = false) {
    if (!draft || !draft.enabled) return;
    const z = fanZones(draft.fans);
    const zones: Record<ZoneId, RgbEffect> = { top: z.top, bottom: z.bottom, pump: structuredClone(draft.pump) };
    // 上下灯带共用一个帧间隔：速度改了哪条都要两条一起改。
    const ids = bothStrips && sel.some((id) => id !== "pump") ? Array.from(new Set<ZoneId>([...sel, "top", "bottom"])) : sel;
    for (const id of ids) {
      mutate(zones[id], id);
      fit(id, zones[id]);
    }
    draft.fans = joinFans(zones.top, zones.bottom, z.flip);
    if (sel.includes("pump")) {
      draft.pump = zones.pump;
      pumpTouched = true;
    }
    redraw();
    if (sel.some((id) => id !== "pump")) {
      if (settle) {
        fansPending = true;
        clearTimeout(fanTimer);
        fanTimer = window.setTimeout(() => commitFans(label), SETTLE_MS);
      } else {
        commitFans(label);
      }
    }
  }

  function commitFans(label: string) {
    clearTimeout(fanTimer);
    fansPending = false;
    if (!draft || !store.config) return;
    const next = structuredClone(store.config);
    next.lighting.fans = structuredClone(draft.fans);
    next.lighting.enabled = draft.enabled;
    commit(next, `风扇${label}`, "light-fans");
  }

  async function sendPump() {
    if (!draft || !store.config) return;
    const next = structuredClone(store.config);
    next.lighting.pump = structuredClone(draft.pump);
    pumpTouched = false;
    await commit(next, "冷头灯框（泵会短暂降速）", "light-pump");
    redraw();
  }

  function revertPump() {
    if (!draft || !sent()) return;
    draft.pump = structuredClone(sent()!.pump);
    pumpTouched = false;
    redraw();
  }

  function applyScene(scene: LightingScene) {
    if (!store.config) return;
    const next = structuredClone(store.config);
    next.lighting.enabled = true;
    const flip = fanZones(next.lighting.fans).flip;
    next.lighting.fans = scene.fans.map((r) => ({ ...structuredClone(r), flip }));
    next.lighting.pump = structuredClone(scene.pump);
    draft = structuredClone(next.lighting);
    for (const id of ZONES) {
      const e = zoneEffect(id);
      if (e) fit(id, e);
    }
    pumpTouched = false;
    redraw();
    commit(next, `场景“${scene.name}”`, "light-scene");
  }

  // ---------- 顶部 ----------
  const enabledSwitch = toggle("由 LConnect Lite 控制灯光", (on) => {
    if (!store.config || !draft) return;
    draft.enabled = on;
    const next = structuredClone(store.config);
    next.lighting.enabled = on;
    commit(next, on ? "开始控制灯光" : "停止控制灯光（设备保持现有灯光）", "light-enabled");
    redraw();
  });
  const sceneRow = h("div", { style: "display: flex; flex-wrap: wrap; align-items: center; gap: 8px" });
  const saveScene = h("button", { type: "button", class: "chip", style: "border-style: dashed; color: var(--muted)" }, "+ 保存为场景");
  saveScene.addEventListener("click", async () => {
    if (!store.config || !draft) return;
    const name = await promptDialog({ title: "保存当前灯效为场景", placeholder: "比如：游戏、看片", confirm: "保存" });
    if (!name) return;
    const next = structuredClone(store.config);
    next.lighting.scenes = [...(next.lighting.scenes ?? []).filter((x) => x.name !== name), { name, fans: structuredClone(draft.fans), pump: structuredClone(draft.pump) }];
    commit(next, `保存场景“${name}”`, "light-scene");
  });
  let scenesKey = "";
  function drawScenes() {
    const user = store.config?.lighting.scenes ?? [];
    const key = JSON.stringify(user.map((x) => x.name));
    if (key === scenesKey) return;
    scenesKey = key;
    const chip = (sc: LightingScene, dot: string, removable: boolean) => {
      const b = h("button", { type: "button", class: "chip" }, h("span", { class: "swatch", style: `background: ${dot}; ${dot === "transparent" ? "box-shadow: inset 0 0 0 1.5px #4a5158" : ""}` }), sc.name);
      b.addEventListener("click", () => applyScene(sc));
      if (!removable) return b;
      b.title = "右键删除";
      b.addEventListener("contextmenu", async (e) => {
        e.preventDefault();
        if (!(await confirmDialog({ title: `删除场景“${sc.name}”？`, body: "只删除这个保存的场景，不影响当前灯效。", confirm: "删除", danger: true })) || !store.config) return;
        const next = structuredClone(store.config);
        next.lighting.scenes = next.lighting.scenes.filter((x) => x.name !== sc.name);
        commit(next, `删除场景“${sc.name}”`, "light-scene");
      });
      return b;
    };
    const dotOf = (sc: LightingScene) => {
      const c = sc.fans[0]?.effect.colors[0];
      return c ? toHex(c) : "var(--surface-3)";
    };
    sceneRow.replaceChildren(
      h("span", { class: "subtitle", style: "margin-right: 2px" }, "场景"),
      ...BUILTIN.map((sc) => chip(sc, sc.dot, false)),
      ...user.map((sc) => chip(sc, dotOf(sc), true)),
      saveScene,
    );
  }

  // ---------- 设备图 ----------
  const chips = ZONES.map((id) => {
    const fx = h("span", { class: "fx" });
    const b = h("button", { type: "button", class: "zone-chip", "aria-pressed": "false" }, h("span", { class: "box" }), ZN[id], fx);
    b.addEventListener("click", () => {
      sel = sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id];
      slot = 0;
      closeList();
      redraw();
    });
    return { b, fx };
  });
  const allBtn = h("button", { type: "button", class: "btn btn-sm btn-ghost", style: "margin-left: auto" }, "全选");
  allBtn.addEventListener("click", () => {
    sel = [...ZONES];
    redraw();
  });
  const flipSwitch = toggle("风扇反装（上下翻转）", (on) => {
    if (!draft || !draft.enabled) return;
    const z = fanZones(draft.fans);
    draft.fans = joinFans(z.top, z.bottom, on);
    redraw();
    commitFans(on ? "上下翻转" : "取消上下翻转");
  });

  const stage = h("div", { class: "stage" });
  const stageInner = h("div", { class: "stage-inner" });
  stage.append(stageInner);
  let leds: { el: SVGElement; glow: SVGElement; kind: "fan" | "pump"; index: number }[] = [];
  let outlineBoxes: Record<ZoneId, SVGRectElement> | null = null;
  let builtFans = 0;

  function buildStage() {
    const n = fanCount();
    builtFans = n;
    const W = 32 + n * 120;
    const H = 334;
    const along = (pts: number[][], count: number) => {
      const seg: number[] = [];
      let total = 0;
      for (let i = 1; i < pts.length; i++) {
        const l = Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]);
        seg.push(l);
        total += l;
      }
      const at = (d: number) => {
        for (let i = 0; i < seg.length; i++) {
          if (d <= seg[i] || i === seg.length - 1) {
            const r = seg[i] ? Math.min(d / seg[i], 1) : 0;
            return [pts[i][0] + (pts[i + 1][0] - pts[i][0]) * r, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * r];
          }
          d -= seg[i];
        }
        return pts[pts.length - 1];
      };
      return Array.from({ length: count }, (_, k) => {
        const a = (k / count) * total, b = ((k + 1) / count) * total;
        return [at(a + 0.5), at((a + b) / 2), at(b - 0.5)].map((p) => `${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(" ");
      });
    };
    const line = (pts: number[][]) => pts.map((p) => p.join(",")).join(" ");
    const outline = s("g", { fill: "none", stroke: "#262a2f", "stroke-width": 1.2 });
    const tracks = s("g", { fill: "none", stroke: "#202327", "stroke-width": 5, "stroke-linejoin": "round", "stroke-linecap": "round" });
    const glow = s("g", { filter: "url(#led-glow)", opacity: 0.8 });
    const crisp = s("g", {});
    leds = [];
    const addLed = (pts: string, kind: "fan" | "pump", index: number) => {
      const g = s("polyline", { points: pts, fill: "none", stroke: "transparent", "stroke-width": 10, "stroke-linecap": "round" });
      const c = s("polyline", { points: pts, fill: "none", stroke: "transparent", "stroke-width": 3.5 });
      glow.append(g);
      crisp.append(c);
      leds.push({ el: c, glow: g, kind, index });
    };
    for (let f = 0; f < n; f++) {
      const x = 16 + f * 120;
      outline.append(s("rect", { x, y: 16, width: 120, height: 120, rx: 5 }), s("circle", { cx: x + 60, cy: 76, r: 46 }));
      const top = [[x + 3, 34], [x + 16, 21], [x + 104, 21], [x + 117, 34]];
      const bot = [[x + 3, 118], [x + 16, 131], [x + 104, 131], [x + 117, 118]];
      tracks.append(s("polyline", { points: line(top) }), s("polyline", { points: line(bot) }));
      along(top, 13).forEach((p, i) => addLed(p, "fan", f * LEDS_PER_FAN + i));
      along(bot, 13).forEach((p, i) => addLed(p, "fan", f * LEDS_PER_FAN + 13 + i));
    }
    const px = W / 2 - 60;
    outline.append(s("rect", { x: px, y: 176, width: 120, height: 120, rx: 9 }), s("rect", { x: px + 20, y: 196, width: 80, height: 80, rx: 2 }));
    // 冷头灯框 24 颗，每边 6 颗；起点和方向按左上角顺时针示意，待实机核对。
    const frame = [[px + 8, 184], [px + 112, 184], [px + 112, 288], [px + 8, 288], [px + 8, 184]];
    tracks.append(s("polyline", { points: line(frame) }));
    along(frame, PUMP_LEDS).forEach((p, i) => addLed(p, "pump", i));
    const fanW = n * 120 + 8;
    const box = (x: number, y: number, w: number, hh: number) => s("rect", { class: "stage-outline", style: "opacity: 0", x, y, width: w, height: hh, rx: 8, fill:"var(--accent)", "fill-opacity": 0.07, stroke: "var(--accent)", "stroke-opacity": 0.7, "stroke-width": 1 });
    outlineBoxes = { top: box(12, 12, fanW, 30), bottom: box(12, 110, fanW, 30), pump: box(px - 6, 170, 132, 132) };
    const svg = s(
      "svg",
      { viewBox: `0 0 ${W} ${H}`, role: "img", "aria-label": "灯效预览" },
      s("defs", {}, s("filter", { id: "led-glow", x: "-20%", y: "-80%", width: "140%", height: "260%" }, s("feGaussianBlur", { stdDeviation: 4 }))),
      outlineBoxes.top,
      outlineBoxes.bottom,
      outlineBoxes.pump,
      outline,
      s("text", { x: W / 2, y: 234, "text-anchor": "middle", fill: "#6f767e", "font-size": 9 }, "屏幕"),
      tracks,
      glow,
      crisp,
      s("text", { x: W / 2, y: 160, "text-anchor": "middle", fill: "var(--muted)", "font-size": 11 }, `UNI FAN TL 无线 ×${n}`),
      s("text", { x: W / 2, y: 322, "text-anchor": "middle", fill: "var(--muted)", "font-size": 11 }, "隐流二代 LCD-S"),
    );
    const pct = (v: number, of: number) => `${((v / of) * 100).toFixed(2)}%`;
    const hit = (id: ZoneId, x: number, y: number, w: number, hh: number) => {
      const b = h("button", { type: "button", class: "stage-hit", "aria-label": ZN[id], style: `left: ${pct(x, W)}; top: ${pct(y, H)}; width: ${pct(w, W)}; height: ${pct(hh, H)}` });
      b.addEventListener("click", (e) => {
        const add = e.ctrlKey || e.metaKey || e.shiftKey;
        if (add) sel = sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id];
        else sel = [id];
        slot = 0;
        closeList();
        redraw();
      });
      return b;
    };
    stageInner.replaceChildren(svg, hit("top", 12, 10, fanW, 34), hit("bottom", 12, 108, fanW, 34), hit("pump", px - 6, 170, 132, 132));
  }

  const stopAnim = animate((t) => {
    for (const l of leds) {
      const c = preview ? ledAt(l.kind === "fan" ? preview.fan : preview.pump, l.index, t) : null;
      const color = c ?? "transparent";
      if (l.el.getAttribute("stroke") !== color) {
        l.el.setAttribute("stroke", color);
        l.glow.setAttribute("stroke", color);
      }
    }
    paintStrip(t);
  });

  /** 预览：参数一变就重新渲染（缓存过的直接用）。 */
  function refreshPreview() {
    if (!draft) return;
    const n = fanCount();
    const l = draft;
    const key = JSON.stringify([l.fans, l.pump, n]);
    if (key === previewKey) return;
    previewKey = key;
    previewFor(l, n)
      .then((p) => {
        if (previewKey === key) preview = p;
      })
      .catch((e) => {
        if (previewKey === key) preview = null;
        console.warn("灯效预览失败", e);
      });
  }

  // ---------- 编辑区 ----------
  const title = h("h2", {});
  const sub = h("span", { class: "small muted" });
  const fxName = h("span", { style: "flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap" });
  const strip = h("span", { class: "strip" });
  const ddBtn = h("button", { type: "button", class: "dropdown-btn", "aria-haspopup": "listbox", "aria-expanded": "false" }, fxName, strip, icon("chevron"));
  const ddList = h("div", { class: "dropdown-list", role: "listbox", "aria-label": "效果" });
  ddList.hidden = true;
  const prevBtn = h("button", { type: "button", class: "btn btn-icon", "aria-label": "上一个效果" }, icon("left"));
  const nextBtn = h("button", { type: "button", class: "btn btn-icon", "aria-label": "下一个效果" }, icon("right"));
  const fxRow = h("div", { class: "irow" }, h("span", { class: "label" }, "效果"), h("div", { class: "control" }, h("div", { class: "dropdown" }, ddBtn, ddList), prevBtn, nextBtn));

  const slotsBox = h("div", { style: "display: flex; align-items: center; gap: 6px; width: 100%" });
  const custom = colorPicker({ label: "自定义颜色", size: 22, onInput: (hex) => setSlotColor(fromHex(hex), true) });
  const dots = PALETTE.map((c) => {
    const b = h("button", { type: "button", class: "color-dot", style: `background: ${toHex(c)}`, "aria-label": `颜色 ${toHex(c)}` });
    b.addEventListener("click", () => setSlotColor(c, false));
    return b;
  });
  const colorRow = h(
    "div",
    { class: "irow", style: "padding: 12px 0" },
    h("span", { class: "label" }, "颜色"),
    h("div", { class: "control", style: "flex-direction: column; align-items: flex-start; gap: 10px" }, slotsBox, h("div", { style: "display: flex; flex-wrap: wrap; gap: 6px" }, ...dots, custom.el)),
  );

  const speedSeg = segmented(SPEEDS.map((n, i) => ({ value: i, label: n })), (v) => apply("速度", (e) => (e.speed = v), false, true), { label: "速度", small: true });
  const brightSeg = segmented(BRIGHTS.map((n, i) => ({ value: i, label: n })), (v) => apply("亮度", (e) => (e.brightness = v)), { label: "亮度", small: true });
  const dirSeg = segmented(
    [
      { value: "Clockwise", label: "顺时针" },
      { value: "CounterClockwise", label: "逆时针" },
    ] as { value: "Clockwise" | "CounterClockwise"; label: string }[],
    (v) => apply("方向", (e) => (e.direction = v)),
    { label: "方向", small: true },
  );
  const speedRow = h("div", { class: "irow" }, h("span", { class: "label" }, "速度"), h("div", { class: "control" }, speedSeg.el));
  const brightRow = h("div", { class: "irow" }, h("span", { class: "label" }, "亮度"), h("div", { class: "control" }, brightSeg.el));
  const dirRow = h("div", { class: "irow" }, h("span", { class: "label" }, "方向"), h("div", { class: "control" }, dirSeg.el));
  const note = h("span", { class: "small", style: "padding: 2px 0 8px 56px; color: var(--faint)" });
  const editBox = h("div", { style: "display: flex; flex-direction: column" }, fxRow, colorRow, speedRow, brightRow, dirRow, note);
  const emptyBox = h("div", { class: "disabled-note" });

  const fanStatus = h("span", { class: "sync-line" }, h("span", { class: "dot dot-ok" }), h("span", {}));
  const pumpDot = h("span", { class: "dot dot-ok" });
  const pumpStatus = h("span", { class: "sync-line", style: "color: var(--text-2)" }, pumpDot, h("span", {}));
  const revertBtn = h("button", { type: "button", class: "btn", style: "flex: 1" }, "还原");
  const sendBtn = h("button", { type: "button", class: "btn btn-primary", style: "flex: 2" }, "发送到冷头");
  const pumpActions = h("div", { style: "display: flex; gap: 8px" }, revertBtn, sendBtn);
  // 这些块会随选择/状态显示或隐藏：出现时淡入一次（display 从 none 变回来才播放）。
  for (const el of [editBox, emptyBox, colorRow, speedRow, dirRow, pumpActions]) el.classList.add("reveal");
  revertBtn.addEventListener("click", revertPump);
  sendBtn.addEventListener("click", sendPump);

  const root = h(
    "div",
    { class: "page" },
    h(
      "header",
      { class: "page-head" },
      h("div", { class: "page-title" }, h("h1", {}, "灯效"), h("span", { class: "subtitle" }, "点灯带选中，可以多选一起改")),
      h("label", { class: "inline-switch", style: "min-height: 44px; color: var(--text-2)" }, "由 LConnect Lite 控制灯光", enabledSwitch),
    ),
    sceneRow,
    h(
      "section",
      { class: "row-grid", style: "align-items: stretch" },
      h(
        "div",
        { class: "card stage-card" },
        h("div", { style: "display: flex; flex-wrap: wrap; align-items: center; gap: 6px" }, ...chips.map((c) => c.b), allBtn),
        stage,
        h("div", { class: "card-head" }, h("span", { class: "small", style: "color: var(--faint)" }, "Ctrl / Shift 点击可多选"), h("label", { class: "inline-switch small muted" }, "风扇反装（上下翻转）", flipSwitch)),
      ),
      h(
        "aside",
        { class: "card inspector", "aria-label": "编辑所选区域" },
        h("div", { class: "inspector-head" }, title, sub),
        emptyBox,
        editBox,
        h("div", { class: "sync-box", role: "status" }, fanStatus, pumpStatus, pumpActions),
      ),
    ),
  );

  // ---------- 效果下拉 ----------
  function modesForSelection(): string[] {
    if (!caps) return [];
    const lists = sel.map((id) => (id === "pump" ? caps!.pump : caps!.fans).map((p) => p.mode));
    if (!lists.length) return [];
    // “关闭”放到最后，不占列表最显眼的位置。
    const modes = lists[0].filter((m) => lists.every((l) => l.includes(m)));
    return [...modes.filter((m) => m !== "Off"), ...modes.filter((m) => m === "Off")];
  }
  const commonValue = <T,>(f: (e: RgbEffect) => T): T | null => {
    const vals = sel.map((id) => zoneEffect(id)).filter((e): e is RgbEffect => e != null).map(f);
    return vals.length && vals.every((v) => same(v, vals[0])) ? vals[0] : null;
  };
  const pickMode = (m: string) => {
    closeList();
    apply(`效果：${modeName(m)}`, (e) => {
      e.mode = m;
      e.disabled = false;
    });
  };
  function openList() {
    const modes = modesForSelection();
    const cur = commonValue((e) => e.mode);
    active = Math.max(0, modes.indexOf(cur ?? ""));
    const hasPump = sel.includes("pump"), hasFan = sel.some((id) => id !== "pump");
    ddList.replaceChildren(
      h("span", { class: "dropdown-note" }, hasFan && hasPump ? `含冷头灯框，只列两边都支持的 ${modes.length} 种` : hasPump ? `冷头灯框 · ${modes.length} 种` : `TL 风扇 · ${modes.length} 种`),
      ...modes.map((m, i) => {
        const b = h("button", { type: "button", role: "option", class: i === active ? "dropdown-item active" : "dropdown-item", "aria-selected": String(m === cur) }, h("span", { class: "check" }, m === cur ? "✓" : ""), modeName(m));
        b.addEventListener("click", () => pickMode(m));
        return b;
      }),
    );
    reveal(ddList);
    open = true;
    ddBtn.setAttribute("aria-expanded", "true");
    (ddList.querySelector('[aria-selected="true"]') as HTMLElement | null)?.scrollIntoView({ block: "nearest" });
  }
  function closeList() {
    if (!open) return;
    open = false;
    conceal(ddList);
    ddBtn.setAttribute("aria-expanded", "false");
  }
  ddBtn.addEventListener("click", () => (open ? closeList() : openList()));
  ddBtn.addEventListener("keydown", (e) => {
    const modes = modesForSelection();
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!open) return openList();
      active = (active + (e.key === "ArrowDown" ? 1 : -1) + modes.length) % modes.length;
      ddList.querySelectorAll(".dropdown-item").forEach((el, i) => el.classList.toggle("active", i === active));
      ddList.querySelectorAll(".dropdown-item")[active]?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter" && open) {
      e.preventDefault();
      pickMode(modes[active]);
    } else if (e.key === "Escape") {
      closeList();
    }
  });
  const onDocClick = (e: MouseEvent) => {
    if (open && !(e.target as Element).closest(".dropdown")) closeList();
  };
  document.addEventListener("click", onDocClick);
  const stepMode = (d: number) => {
    const modes = modesForSelection();
    if (!modes.length) return;
    const i = modes.indexOf(commonValue((e) => e.mode) ?? "");
    pickMode(modes[(i < 0 ? 0 : i + d + modes.length) % modes.length]);
  };
  prevBtn.addEventListener("click", () => stepMode(-1));
  nextBtn.addEventListener("click", () => stepMode(1));

  // ---------- 颜色 ----------
  // TL 的“静态”“呼吸”是每只风扇一种颜色。默认按“统一颜色”显示一个色块、改一次三只一起变；
  // 只有主动切到“每只风扇”才显示带编号的三个色块。
  let split: boolean | null = null;
  let splitFor = "";
  const isPerFan = (id: ZoneId, e: RgbEffect) => id !== "pump" && !!paramsOf(id, e.mode)?.per_fan_colors;
  const perFanZones = () => sel.filter((id) => {
    const e = zoneEffect(id);
    return e != null && isPerFan(id, e);
  });
  function splitOn() {
    const key = JSON.stringify(sel);
    if (key !== splitFor) {
      splitFor = key;
      split = null;
    }
    return split ?? false;
  }
  /** 统一模式下，三只风扇当前颜色其实不一样（比如 L-Connect 设的或之前分开设过）。 */
  const mixedFans = () => perFanZones().some((id) => new Set(zoneEffect(id)!.colors.map(toHex)).size > 1);
  function slotCount() {
    const s = splitOn();
    return Math.max(0, ...sel.map((id) => {
      const e = zoneEffect(id);
      if (!e) return 0;
      return isPerFan(id, e) && !s ? 1 : colorCount(id, e);
    }));
  }
  function setSlotColor(c: Rgb, settle: boolean) {
    const s = splitOn();
    apply("颜色", (e, id) => {
      if (isPerFan(id, e) && !s) e.colors = e.colors.map(() => c);
      else if (slot < colorCount(id, e)) e.colors[slot] = c;
    }, settle);
  }
  const splitSeg = segmented(
    [
      { value: "same", label: "统一颜色" },
      { value: "each", label: "每只风扇" },
    ] as { value: "same" | "each"; label: string }[],
    (v) => {
      split = v === "each";
      slot = 0;
      // 切回统一：三只都用第一个颜色。
      if (!split) apply("统一颜色", (e, id) => {
        if (isPerFan(id, e)) e.colors = e.colors.map(() => e.colors[0]);
      });
      else redraw();
    },
    { label: "风扇颜色", small: true },
  );
  splitSeg.el.style.width = "170px";
  let slotsKey = "";
  function drawSlots() {
    const s = splitOn();
    const n = slotCount();
    if (slot >= n) slot = 0;
    const first = sel.map((id) => zoneEffect(id)).find((e) => e && e.colors.length) ?? null;
    const perFan = perFanZones().length > 0;
    splitSeg.el.hidden = !perFan;
    splitSeg.set(s ? "each" : "same");
    const numbered = perFan && s;
    const mixed = perFan && !s && mixedFans();
    // 当前色块的颜色不在预设里时，自定义取色按钮显示为选中。
    const cur = first?.colors[slot];
    if (cur) custom.set(toHex(cur), !PALETTE.some((p) => toHex(p) === toHex(cur)));
    const key = JSON.stringify([n, slot, first?.colors, numbered, perFan, mixed]);
    if (key === slotsKey) return;
    slotsKey = key;
    // 只换色块，分段选择留在原地：挪动它会打断滑块的过渡动画。
    for (const c of [...slotsBox.children]) if (c !== splitSeg.el) c.remove();
    if (splitSeg.el.parentNode !== slotsBox) slotsBox.append(splitSeg.el);
    splitSeg.el.before(
      ...Array.from({ length: n }, (_, i) => {
        const c = first?.colors[i] ?? PALETTE[i % PALETTE.length];
        const label = numbered ? `风扇 ${i + 1} 的颜色` : n > 1 ? `颜色 ${i + 1}` : perFan ? "所有风扇的颜色" : "颜色";
        const b = h("button", { type: "button", class: "color-slot", "aria-pressed": String(i === slot), style: `background: ${toHex(c)}`, "aria-label": label, title: label }, numbered ? h("span", { class: "slot-num" }, String(i + 1)) : null);
        b.addEventListener("click", () => {
          slot = i;
          drawSlots();
        });
        return b;
      }),
      ...(mixed ? [h("span", { class: "small", style: "color: var(--faint); line-height: 1.3" }, "三只现在颜色不同，选一个颜色会统一")] : []),
    );
  }

  /** 下拉按钮里的预览条：风扇取各只的上灯带连成一条，冷头取整圈。 */
  function paintStrip(t: number) {
    const id = sel[0];
    if (!id || !preview) return;
    const kids = strip.children;
    const n = id === "pump" ? PUMP_LEDS : fanCount() * 13;
    if (kids.length !== n) strip.replaceChildren(...Array.from({ length: n }, () => h("span")));
    for (let i = 0; i < n; i++) {
      const c = id === "pump" ? ledAt(preview.pump, i, t) : ledAt(preview.fan, Math.floor(i / 13) * LEDS_PER_FAN + (id === "bottom" ? 13 : 0) + (i % 13), t);
      (kids[i] as HTMLElement).style.background = c ?? "transparent";
    }
  }

  // ---------- 绘制 ----------
  function redraw() {
    if (!draft) return;
    if (builtFans !== fanCount()) buildStage();
    enabledSwitch.checked = draft.enabled;
    flipSwitch.checked = fanZones(draft.fans).flip;
    flipSwitch.disabled = !draft.enabled;
    stage.style.opacity = draft.enabled ? "" : "0.45";
    for (const [i, id] of ZONES.entries()) {
      chips[i].b.setAttribute("aria-pressed", String(sel.includes(id)));
      const e = zoneEffect(id);
      chips[i].fx.textContent = e ? modeName(e.disabled ? "Off" : e.mode) : "";
      // 选中框淡入淡出（CSS 过渡），值没变时不重设。
      if (outlineBoxes) {
        const on = sel.includes(id) && draft.enabled ? "1" : "0";
        if (outlineBoxes[id].style.opacity !== on) outlineBoxes[id].style.opacity = on;
      }
    }
    drawScenes();

    const editable = draft.enabled && sel.length > 0 && caps != null;
    editBox.hidden = !editable;
    emptyBox.hidden = editable;
    emptyBox.textContent = !draft.enabled ? "灯光控制已关闭，设备保持现有灯光。打开右上角的开关后可以编辑。" : !caps ? "正在读取灯效能力…" : "点左边的灯带或冷头";
    title.textContent = sel.length ? sel.map((id) => ZN[id]).join(" + ") : "编辑";
    sub.textContent = sel.length > 1 ? `改动会同时作用到这 ${sel.length} 个区域` : sel.length ? "改动只作用到这个区域" : "没有选中区域";

    if (editable) {
      const mode = commonValue((e) => e.mode);
      const modes = Array.from(new Set(sel.map((id) => modeName(zoneEffect(id)!.mode))));
      fxName.textContent = mode ? modeName(mode) : `混合（${modes.join("、")}）`;
      const ps = sel.map((id) => paramsOf(id, zoneEffect(id)!.mode));
      colorRow.hidden = slotCount() === 0;
      drawSlots();
      speedRow.hidden = !ps.some((p) => p?.supports_speed);
      speedSeg.set(commonValue((e) => e.speed));
      brightSeg.set(commonValue((e) => e.brightness));
      dirRow.hidden = !ps.every((p) => p && p.directions.length > 1);
      dirSeg.set(commonValue((e) => e.direction));
      note.textContent = sel.some((id) => id !== "pump") && !speedRow.hidden ? "上下灯带共用一个速度（同一个无线设备）" : "";
    }

    const fanSpan = fanStatus.lastElementChild!;
    fanSpan.textContent = fansPending ? "风扇：正在应用…" : !draft.enabled ? "灯光：未控制" : "风扇：改动实时生效";
    const dirty = pumpDirty();
    pumpDot.className = dirty ? "dot dot-warn" : "dot dot-ok";
    pumpStatus.lastElementChild!.textContent = dirty ? "冷头灯框有改动未发送（发送时泵会降速约 7 秒）" : "冷头灯框：已同步";
    pumpActions.hidden = !dirty;
    refreshPreview();
  }

  async function loadCaps() {
    try {
      caps = await api.getLightingCaps();
      if (draft) for (const id of ZONES) fitZone(id);
      redraw();
    } catch (e) {
      toast(`读取灯效能力失败：${e}`, { error: true, key: "caps" });
    }
  }
  /** 配置里的颜色数可能和效果要求不符（比如手改过配置文件）：显示前先修正，不写回。 */
  function fitZone(id: ZoneId) {
    if (!draft) return;
    if (id === "pump") return fit(id, draft.pump);
    const z = fanZones(draft.fans);
    fit("top", z.top);
    fit("bottom", z.bottom);
    draft.fans = joinFans(z.top, z.bottom, z.flip);
  }

  function update() {
    const cfg = store.config?.lighting;
    if (cfg && draft && !fansPending) {
      // 撤销、托盘或别处改了配置：风扇跟着后台；冷头灯框没动过才跟着。
      if (!same(cfg.fans, draft.fans) || cfg.enabled !== draft.enabled) {
        draft.fans = structuredClone(cfg.fans);
        draft.enabled = cfg.enabled;
      }
      if (!pumpTouched && !same(cfg.pump, draft.pump)) draft.pump = structuredClone(cfg.pump);
    } else if (cfg && !draft) {
      draft = structuredClone(cfg);
    }
    if (!caps && !store.offline) loadCaps();
    redraw();
  }

  if (draft) redraw();
  return {
    root,
    update,
    dispose() {
      stopAnim();
      document.removeEventListener("click", onDocClick);
      if (fansPending) commitFans("颜色");
    },
    async beforeLeave() {
      if (!pumpDirty()) return true;
      const send = await confirmDialog({ title: "冷头灯框的改动还没发送", body: "发送时泵会短暂回到默认转速（约 7 秒）。不发送的话，冷头灯框保持原来的效果。", confirm: "发送并离开", cancel: "不发送" });
      if (send) await sendPump();
      else revertPump();
      return true;
    },
  };
}
