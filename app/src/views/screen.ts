import { api, type Config, type LcdMode } from "../api";
import { h } from "../dom";
import { saveConfig, store } from "../store";
import type { View } from "./view";

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const MODES: [LcdMode, string][] = [
  ["unmanaged", "不控制"],
  ["dashboard", "仪表盘"],
  ["image", "图片"],
  ["gif", "GIF"],
];
const PREVIEW_INTERVAL = 2000;

export function screen(): View {
  let base: Config | null = null;
  let draft: Config | null = null;
  let message = "";

  const modeButtons = MODES.map(([m, label]) => h("button", { type: "button", class: "btn", "data-mode": m, "aria-pressed": "false" }, label));
  const pathText = h("code", { class: "mono small path" });
  const pickBtn = h("button", { type: "button", class: "btn btn-small" }, "选择文件…");
  const fileRow = h("div", { class: "field" }, h("span", { class: "field-label" }, "文件"), pickBtn, pathText);
  const brightness = h("input", { type: "range", id: "lcd-bright", min: 0, max: 100, step: 5 });
  const brightnessOut = h("output", { for: "lcd-bright", class: "mono" });
  const rotButtons = [0, 90, 180, 270].map((r) => h("button", { type: "button", class: "btn btn-small", "data-rot": r, "aria-pressed": "false" }, `${r}°`));
  const fps = h("input", { type: "number", id: "lcd-fps", class: "num", min: 1, max: 30, step: 1 });
  const fpsRow = h("div", { class: "field" }, h("label", { for: "lcd-fps" }, "最高帧率"), fps, h("span", { class: "muted small" }, "fps"));
  const clock = h("input", { type: "checkbox", id: "lcd-clock" });
  const previewImg = h("img", { class: "lcd-preview", alt: "屏幕当前画面" });
  const previewEmpty = h("div", { class: "lcd-preview lcd-empty" }, "还没有推送过画面");
  const describe = h("p", { class: "small" });
  const lcdError = h("p", { class: "small error-text", role: "status" });
  const statusText = h("span", { class: "muted" });
  const revertBtn = h("button", { type: "button", class: "btn" }, "撤销修改");
  const applyBtn = h("button", { type: "button", class: "btn btn-primary" }, "应用");

  const root = h(
    "div",
    { class: "page" },
    h(
      "header",
      { class: "page-head" },
      h("div", { class: "stack-tight" }, h("h1", {}, "冷头屏幕"), h("span", { class: "subtitle" }, "480 × 480。画面在电脑上裁切、旋转、压缩后推送；选“不控制”时保持设备现有画面。")),
      h("div", { class: "btn-row", role: "group", "aria-label": "屏幕内容" }, ...modeButtons),
    ),
    h(
      "section",
      { class: "split" },
      h(
        "div",
        { class: "card card-light" },
        h("h2", {}, "设置"),
        fileRow,
        h("div", { class: "field" }, h("label", { for: "lcd-bright" }, "亮度"), brightness, brightnessOut),
        h("div", { class: "field" }, h("span", { class: "field-label" }, "旋转"), h("div", { class: "btn-row", role: "group", "aria-label": "旋转" }, ...rotButtons)),
        fpsRow,
        h("label", { class: "check", for: "lcd-clock" }, clock, "离线时钟（电脑没在推画面时屏幕显示时钟）"),
      ),
      h("div", { class: "card card-preview" }, h("h2", {}, "当前画面"), previewEmpty, previewImg, describe, lcdError),
    ),
    h("footer", { class: "action-bar" }, statusText, h("div", { class: "btn-row" }, revertBtn, applyBtn)),
  );
  previewImg.hidden = true;

  const dirty = () => draft != null && base != null && !same(draft, base);

  function draw() {
    if (!draft) return;
    const l = draft.lcd;
    modeButtons.forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.mode === l.mode)));
    const media = l.mode === "image" || l.mode === "gif";
    fileRow.hidden = !media;
    fpsRow.hidden = l.mode !== "gif";
    pathText.textContent = l.media_path || "（未选择）";
    brightness.value = String(l.brightness);
    brightnessOut.textContent = `${l.brightness}%`;
    rotButtons.forEach((b) => b.setAttribute("aria-pressed", String(Number(b.dataset.rot) === l.rotation)));
    if (document.activeElement !== fps) fps.value = String(l.max_fps);
    clock.checked = l.offline_clock;
    drawStatus();
  }

  function drawStatus() {
    const isDirty = dirty();
    revertBtn.toggleAttribute("disabled", !isDirty);
    applyBtn.toggleAttribute("disabled", !isDirty);
    const needFile = draft && (draft.lcd.mode === "image" || draft.lcd.mode === "gif") && !draft.lcd.media_path;
    statusText.textContent = message || (needFile ? "先选择一个文件" : isDirty ? "有未应用的修改" : draft ? "已应用" : "等待后台服务…");
  }

  modeButtons.forEach((b) =>
    b.addEventListener("click", () => {
      if (!draft) return;
      const next = b.dataset.mode as LcdMode;
      // 图片和 GIF 用不同的文件：换类型时清掉不匹配的路径。
      const isGif = draft.lcd.media_path.toLowerCase().endsWith(".gif");
      if ((next === "gif" && !isGif) || (next === "image" && isGif)) draft.lcd.media_path = "";
      draft.lcd.mode = next;
      draw();
    }),
  );
  pickBtn.addEventListener("click", async () => {
    if (!draft) return;
    try {
      const path = await api.pickMedia(draft.lcd.mode === "gif" ? "gif" : "image");
      if (path) draft.lcd.media_path = path;
      message = "";
    } catch (e) {
      message = `选择文件失败：${e}`;
    }
    draw();
  });
  brightness.addEventListener("input", () => {
    if (!draft) return;
    draft.lcd.brightness = Number(brightness.value);
    draw();
  });
  rotButtons.forEach((b) =>
    b.addEventListener("click", () => {
      if (!draft) return;
      draft.lcd.rotation = Number(b.dataset.rot);
      draw();
    }),
  );
  fps.addEventListener("change", () => {
    if (!draft) return;
    draft.lcd.max_fps = Math.min(30, Math.max(1, Number(fps.value) || 20));
    draw();
  });
  clock.addEventListener("change", () => {
    if (!draft) return;
    draft.lcd.offline_clock = clock.checked;
    drawStatus();
  });
  revertBtn.addEventListener("click", () => {
    draft = base ? structuredClone(base) : null;
    message = "";
    draw();
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
    draw();
    refreshPreview();
  });

  let lastPreview = 0;
  async function refreshPreview() {
    lastPreview = Date.now();
    try {
      const s = await api.getLcdState();
      describe.textContent = s.description;
      lcdError.textContent = s.error ?? "";
      if (s.preview_jpeg_base64) {
        previewImg.src = `data:image/jpeg;base64,${s.preview_jpeg_base64}`;
        previewImg.hidden = false;
        previewEmpty.hidden = true;
      }
    } catch {
      describe.textContent = "";
    }
  }

  function update() {
    if (store.config && !same(base, store.config)) {
      const keep = dirty();
      base = structuredClone(store.config);
      if (!keep) {
        draft = structuredClone(base);
        draw();
      }
    }
    if (!store.offline && Date.now() - lastPreview >= PREVIEW_INTERVAL) refreshPreview();
    drawStatus();
  }

  if (store.config) {
    base = structuredClone(store.config);
    draft = structuredClone(base);
  }
  draw();
  return { root, update };
}
