import { api, type Config, type EffectParams, type LightingCaps, type Rgb, type RgbEffect, type RgbScope } from "../api";
import { h } from "../dom";
import { modeName } from "../modes";
import { saveConfig, store } from "../store";
import type { View } from "./view";

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const toHex = ([r, g, b]: Rgb) => `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
const fromHex = (hex: string): Rgb => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as Rgb;
const PALETTE: Rgb[] = [[255, 120, 0], [0, 140, 255], [255, 0, 90], [0, 220, 120]];
const SPEED_NAMES = ["最慢", "慢", "中", "快", "最快"];
const BRIGHT_NAMES = ["很暗", "暗", "中", "亮", "最亮"];

/** 让颜色数量落在效果要求的范围里：不够就补默认色，多了就截掉。 */
function fitColors(effect: RgbEffect, p: EffectParams | undefined, fanCount: number) {
  if (!p) return;
  const want = p.per_fan_colors ? fanCount : Math.min(Math.max(effect.colors.length, p.min_colors, p.max_colors ? 1 : 0), p.max_colors);
  while (effect.colors.length < want) effect.colors.push(effect.colors.at(-1) ?? PALETTE[effect.colors.length % PALETTE.length]);
  if (effect.colors.length > want) effect.colors.length = Math.max(want, p.max_colors === 0 ? effect.colors.length : want);
  if (!p.directions.includes(effect.direction)) effect.direction = "Clockwise";
}

/** 一个效果的编辑器。`onChange` 在任何字段改动后调用；结构变化（换效果、增减颜色）时整块重建。 */
function effectEditor(effect: RgbEffect, params: EffectParams[], fanCount: number, idPrefix: string, onChange: () => void): HTMLElement {
  const box = h("div", { class: "effect-editor" });
  const p = () => params.find((x) => x.mode === effect.mode);

  function build() {
    const cur = p();
    const select = h("select", { id: `${idPrefix}-mode`, class: "select" });
    for (const x of params) {
      const opt = h("option", { value: x.mode }, modeName(x.mode));
      if (x.mode === effect.mode) opt.selected = true;
      select.append(opt);
    }
    select.addEventListener("change", () => {
      effect.mode = select.value;
      fitColors(effect, p(), fanCount);
      build();
      onChange();
    });

    const rows: HTMLElement[] = [h("div", { class: "field" }, h("label", { for: `${idPrefix}-mode` }, "效果"), select)];

    if (cur && cur.max_colors > 0) {
      const swatches = h("div", { class: "swatches" });
      effect.colors.forEach((c, i) => {
        const input = h("input", { type: "color", value: toHex(c), "aria-label": cur.per_fan_colors ? `风扇 ${i + 1} 颜色` : `颜色 ${i + 1}` });
        input.addEventListener("input", () => {
          effect.colors[i] = fromHex(input.value);
          onChange();
        });
        swatches.append(cur.per_fan_colors ? h("label", { class: "swatch-label" }, input, h("span", { class: "small muted" }, `风扇 ${i + 1}`)) : input);
      });
      if (!cur.per_fan_colors) {
        const add = h("button", { type: "button", class: "btn btn-small", disabled: effect.colors.length >= cur.max_colors, "aria-label": "加一种颜色" }, "+");
        const del = h("button", { type: "button", class: "btn btn-small", disabled: effect.colors.length <= Math.max(cur.min_colors, 1), "aria-label": "去掉最后一种颜色" }, "−");
        add.addEventListener("click", () => {
          effect.colors.push(PALETTE[effect.colors.length % PALETTE.length]);
          build();
          onChange();
        });
        del.addEventListener("click", () => {
          effect.colors.pop();
          build();
          onChange();
        });
        swatches.append(add, del);
      }
      rows.push(h("div", { class: "field" }, h("span", { class: "field-label" }, "颜色"), swatches));
    }

    if (cur?.supports_speed) rows.push(slider("速度", `${idPrefix}-speed`, effect.speed, SPEED_NAMES, (v) => ((effect.speed = v), onChange())));
    rows.push(slider("亮度", `${idPrefix}-bright`, Math.min(effect.brightness, 4), BRIGHT_NAMES, (v) => ((effect.brightness = v), onChange())));

    if (cur && cur.directions.length > 0) {
      const btns = cur.directions.map((d) => {
        const b = h("button", { type: "button", class: "btn btn-small", "aria-pressed": String(effect.direction === d) }, d === "Clockwise" ? "顺时针" : "逆时针");
        b.addEventListener("click", () => {
          effect.direction = d;
          build();
          onChange();
        });
        return b;
      });
      rows.push(h("div", { class: "field" }, h("span", { class: "field-label" }, "方向"), h("div", { class: "btn-row", role: "group", "aria-label": "方向" }, ...btns)));
    }
    box.replaceChildren(...rows);
  }

  build();
  return box;
}

function slider(label: string, id: string, value: number, names: string[], set: (v: number) => void) {
  const input = h("input", { type: "range", id, min: 0, max: 4, step: 1, value });
  const out = h("output", { for: id, class: "small" }, names[value]);
  input.addEventListener("input", () => {
    out.textContent = names[Number(input.value)];
    set(Number(input.value));
  });
  return h("div", { class: "field" }, h("label", { for: id }, label), input, out);
}

export function lighting(): View {
  let base: Config | null = null;
  let draft: Config | null = null;
  let caps: LightingCaps | null = null;
  let message = "";

  const enabled = h("input", { type: "checkbox", id: "light-on" });
  const splitButtons = (["All", "Split"] as const).map((k) =>
    h("button", { type: "button", class: "btn btn-small", "data-split": k, "aria-pressed": "false" }, k === "All" ? "整体" : "上下分区"),
  );
  const flip = h("input", { type: "checkbox", id: "light-flip" });
  const fanEditors = h("div", { class: "region-list" });
  const pumpEditor = h("div");
  const fanTitle = h("h2", {}, "风扇");
  const statusText = h("span", { class: "muted" });
  const revertBtn = h("button", { type: "button", class: "btn" }, "撤销修改");
  const applyBtn = h("button", { type: "button", class: "btn btn-primary" }, "应用");

  const root = h(
    "div",
    { class: "page" },
    h(
      "header",
      { class: "page-head" },
      h("div", { class: "stack-tight" }, h("h1", {}, "灯效"), h("span", { class: "subtitle" }, "效果在电脑上逐帧算好后下发；风扇确认生效后才算成功，冷头每次推送会短暂重新初始化。")),
      h("label", { class: "check toggle", for: "light-on" }, enabled, "由 LConnect Lite 控制灯效"),
    ),
    h(
      "section",
      { class: "split" },
      h(
        "div",
        { class: "card card-light" },
        h("div", { class: "card-head" }, fanTitle, h("div", { class: "btn-row", role: "group", "aria-label": "分区" }, ...splitButtons)),
        fanEditors,
        h("label", { class: "check", for: "light-flip" }, flip, "上下翻转（风扇装反时用）"),
      ),
      h("div", { class: "card card-light" }, h("h2", {}, "冷头灯圈"), pumpEditor),
    ),
    h("footer", { class: "action-bar" }, statusText, h("div", { class: "btn-row" }, revertBtn, applyBtn)),
  );

  const dirty = () => draft != null && base != null && !same(draft, base);
  const changed = () => drawStatus();

  function drawStatus() {
    const isDirty = dirty();
    revertBtn.toggleAttribute("disabled", !isDirty);
    applyBtn.toggleAttribute("disabled", !isDirty);
    statusText.textContent = message || (isDirty ? "有未应用的修改" : draft ? (draft.lighting.enabled ? "已应用" : "未启用：设备保持现有灯光") : "等待后台服务…");
  }

  function build() {
    if (!draft || !caps) return;
    const l = draft.lighting;
    const fanCount = caps.fan_count;
    enabled.checked = l.enabled;
    const isSplit = l.fans.some((r) => r.effect.scope !== "All");
    splitButtons.forEach((b) => b.setAttribute("aria-pressed", String((b.dataset.split === "Split") === isSplit)));
    flip.checked = l.fans.some((r) => r.flip);
    fanTitle.textContent = `风扇 · TL × ${fanCount}`;
    fanEditors.replaceChildren(
      ...l.fans.map((r, i) => {
        fitColors(r.effect, caps!.fans.find((x) => x.mode === r.effect.mode), fanCount);
        const title = r.effect.scope === "Top" ? "上弧" : r.effect.scope === "Bottom" ? "下弧" : "";
        return h("div", { class: "region" }, title && h("h3", {}, title), effectEditor(r.effect, caps!.fans, fanCount, `fan${i}`, changed));
      }),
    );
    fitColors(l.pump, caps.pump.find((x) => x.mode === l.pump.mode), 1);
    pumpEditor.replaceChildren(effectEditor(l.pump, caps.pump, 1, "pump", changed));
    drawStatus();
  }

  enabled.addEventListener("change", () => {
    if (!draft) return;
    draft.lighting.enabled = enabled.checked;
    drawStatus();
  });
  flip.addEventListener("change", () => {
    if (!draft) return;
    draft.lighting.fans.forEach((r) => (r.flip = flip.checked));
    drawStatus();
  });
  splitButtons.forEach((b) =>
    b.addEventListener("click", () => {
      if (!draft) return;
      const first = draft.lighting.fans[0]?.effect;
      const fl = flip.checked;
      if (b.dataset.split === "All") {
        draft.lighting.fans = [{ effect: { ...structuredClone(first), scope: "All" }, flip: fl }];
      } else {
        const mk = (scope: RgbScope) => ({ effect: { ...structuredClone(first), scope }, flip: fl });
        draft.lighting.fans = [mk("Top"), mk("Bottom")];
      }
      build();
    }),
  );
  revertBtn.addEventListener("click", () => {
    draft = base ? structuredClone(base) : null;
    message = "";
    build();
  });
  applyBtn.addEventListener("click", async () => {
    if (!draft) return;
    applyBtn.setAttribute("disabled", "");
    try {
      await saveConfig(draft);
      base = store.config ? structuredClone(store.config) : null;
      draft = base ? structuredClone(base) : null;
      message = "";
    } catch (e) {
      message = `应用失败：${e}`;
    }
    build();
  });

  let loadingCaps = false;
  async function loadCaps() {
    if (loadingCaps) return;
    loadingCaps = true;
    try {
      caps = await api.getLightingCaps();
      build();
    } catch (e) {
      message = `读取灯效能力失败：${e}`;
      drawStatus();
    } finally {
      loadingCaps = false;
    }
  }

  function update() {
    // 后台配置变了且没有未应用的修改：跟着更新。
    if (store.config && !same(base, store.config)) {
      const keep = dirty();
      base = structuredClone(store.config);
      if (!keep) {
        draft = structuredClone(base);
        build();
      }
    }
    if (!caps && !store.offline) loadCaps();
    drawStatus();
  }

  if (store.config) {
    base = structuredClone(store.config);
    draft = structuredClone(base);
  }
  drawStatus();
  return { root, update };
}
