import type { Config, Mode, Point, TempSource } from "../api";
import { renderChart, scales, toSvgPoint, type ChartOptions } from "../chart";
import { activePreset, applyPreset, evalCurve, PRESETS } from "../curve";
import { fmt, h, s } from "../dom";
import { saveConfig, store } from "../store";
import type { View } from "./view";

type Tab = "fan" | "pump";

/** 每条曲线的坐标范围、步长和硬件限制。 */
const SPEC = {
  fan: { yMin: 20, yMax: 100, yStep: 1, axis: { min: 0, max: 100, ticks: [0, 20, 40, 60, 80, 100] }, unit: "%", name: "风扇" },
  pump: { yMin: 1600, yMax: 3200, yStep: 10, axis: { min: 1600, max: 3200, ticks: [1600, 2000, 2400, 2800, 3200] }, unit: "rpm", name: "泵" },
} as const;
/** 横轴：水温一般在 20–55 °C；GPU 温度范围更大。 */
const X_COOLANT = { min: 20, max: 55, step: 0.5, gap: 0.5, ticks: [20, 25, 30, 35, 40, 45, 50, 55], name: "水温" };
const X_GPU = { min: 30, max: 90, step: 1, gap: 1, ticks: [30, 40, 50, 60, 70, 80, 90], name: "GPU 温度" };
const MODE_NAMES: Record<Mode, string> = { fixed: "固定", curve: "曲线", motherboard: "跟随主板" };
const CHART_W = 720;
const CHART_H = 340;

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);
const snap = (v: number, step: number) => Math.round(v / step) * step;
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

export function curves(): View {
  /** 草稿所基于的后台配置；草稿和它不同才算“有未应用的修改”。 */
  let base: Config | null = null;
  let draft: Config | null = null;
  let tab: Tab = "fan";
  let selected: number | null = null;
  let dragging: number | null = null;
  let message = "";

  const pts = (): Point[] => (draft ? draft.cooling[tab === "fan" ? "fan_curve" : "pump_curve"].points : []);
  const setPts = (p: Point[]) => {
    if (draft) draft.cooling[tab === "fan" ? "fan_curve" : "pump_curve"].points = p;
  };
  const source = (): TempSource => (tab === "fan" ? draft?.cooling.fan_source ?? "coolant" : "coolant");
  const X = () => (source() === "gpu" ? X_GPU : X_COOLANT);
  const dirty = () => draft != null && base != null && !same(draft, base);
  const reset = () => {
    base = store.config ? structuredClone(store.config) : null;
    draft = base ? structuredClone(base) : null;
  };

  // ---------- 结构 ----------
  const modeButtons = (["fixed", "curve", "motherboard"] as Mode[]).map((m) =>
    h("button", { type: "button", class: "btn", "data-mode": m, "aria-pressed": "false" }, MODE_NAMES[m]),
  );
  const sourceButtons = (["coolant", "gpu"] as TempSource[]).map((src) =>
    h("button", { type: "button", class: "btn btn-small", "data-source": src, "aria-pressed": "false" }, src === "gpu" ? "按 GPU 温度" : "按水温"),
  );
  const sourceGroup = h("div", { class: "btn-row", role: "group", "aria-label": "风扇曲线温度源" }, ...sourceButtons);
  const cpuBoost = h("input", { type: "range", id: "cpu-boost", min: 0, max: 30, step: 1 });
  const cpuBoostOut = h("output", { for: "cpu-boost", class: "mono" });
  const gpuBoost = h("input", { type: "range", id: "gpu-boost", min: 0, max: 30, step: 1 });
  const gpuBoostOut = h("output", { for: "gpu-boost", class: "mono" });
  const perFan = h("div", { class: "per-fan" });
  const tabButtons = (["fan", "pump"] as Tab[]).map((t) =>
    h("button", { type: "button", class: "btn", "data-tab": t, "aria-pressed": "false" }, t === "fan" ? "风扇曲线" : "泵曲线"),
  );
  const presetButtons = PRESETS.map((p) => h("button", { type: "button", class: "btn btn-small" }, `载入${p.name}`));
  const svg = s("svg", {
    class: "chart chart-edit",
    tabindex: 0,
    role: "img",
    "aria-label": "曲线编辑区：拖动点调整，双击空白处添加，选中后按方向键微调、Delete 删除",
  });
  const tbody = h("tbody");
  const addBtn = h("button", { type: "button", class: "btn btn-small" }, "添加点");
  const fixedFan = h("input", { type: "range", id: "fixed-fan", min: 20, max: 100, step: 1 });
  const fixedFanOut = h("output", { for: "fixed-fan", class: "mono" });
  const fixedPump = h("input", { type: "range", id: "fixed-pump", min: 1600, max: 3200, step: 50 });
  const fixedPumpOut = h("output", { for: "fixed-pump", class: "mono" });
  const slew = h("input", { type: "number", id: "slew", min: 0.5, max: 100, step: 0.5, class: "num" });
  const readings = h("dl", { class: "readings" });
  const statusText = h("span", { class: "muted" });
  const revertBtn = h("button", { type: "button", class: "btn" }, "撤销修改");
  const applyBtn = h("button", { type: "button", class: "btn btn-primary" }, "应用");

  const root = h(
    "div",
    { class: "page" },
    h(
      "header",
      { class: "page-head" },
      h(
        "div",
        { class: "stack-tight" },
        h("h1", {}, "风扇与泵"),
        h("span", { class: "subtitle" }, "曲线模式按温度查表；固定和跟随主板模式下泵使用右侧的固定值"),
      ),
      h("div", { class: "btn-row", role: "group", "aria-label": "控制方式" }, ...modeButtons),
    ),
    h(
      "section",
      { class: "split split-editor" },
      h(
        "div",
        { class: "card card-editor" },
        h(
          "div",
          { class: "card-head" },
          h("div", { class: "btn-row", role: "group", "aria-label": "选择曲线" }, ...tabButtons),
          h("div", { class: "btn-row" }, ...presetButtons),
        ),
        sourceGroup,
        svg,
        h("p", { class: "hint" }, "拖动点调整 · 双击空白处添加点 · 选中后方向键微调、Delete 删除"),
        h(
          "div",
          { class: "table-wrap" },
          h(
            "table",
            { class: "points" },
            h("thead", {}, h("tr", {}, h("th", {}, "#"), h("th", { class: "th-x" }, "水温 (°C)"), h("th", { class: "th-value" }, "数值"), h("th", {}, ""))),
            tbody,
          ),
        ),
        h("div", { class: "btn-row" }, addBtn),
      ),
      h(
        "aside",
        { class: "card card-side" },
        h("h2", {}, "实时"),
        readings,
        h("h2", {}, "固定模式"),
        h("div", { class: "field" }, h("label", { for: "fixed-fan" }, "风扇"), fixedFan, fixedFanOut),
        h("div", { class: "field" }, h("label", { for: "fixed-pump" }, "泵"), fixedPump, fixedPumpOut),
        h("h2", {}, "平滑"),
        h("div", { class: "field" }, h("label", { for: "slew" }, "风扇变化限速"), slew, h("span", { class: "muted" }, "%/秒")),
        h("h2", {}, "前馈"),
        h("p", { class: "hint" }, "负载上来时提前加速：占用 10% 以上按比例叠加，满载时加满。只在曲线模式生效。"),
        h("div", { class: "field" }, h("label", { for: "cpu-boost" }, "CPU 满载加"), cpuBoost, cpuBoostOut),
        h("div", { class: "field" }, h("label", { for: "gpu-boost" }, "GPU 满载加"), gpuBoost, gpuBoostOut),
        h("h2", {}, "单个风扇"),
        h("p", { class: "hint" }, "在全局输出上加减，或固定为某个值。"),
        perFan,
      ),
    ),
    h("footer", { class: "action-bar" }, statusText, h("div", { class: "btn-row" }, revertBtn, applyBtn)),
  );

  // ---------- 绘制 ----------
  function chartOptions(): ChartOptions {
    const spec = SPEC[tab];
    const st = store.status;
    const now = tab === "fan" ? st?.fan_percent : st?.pump_rpm;
    const temp = sourceTemp();
    return {
      width: CHART_W,
      height: CHART_H,
      x: { min: X().min, max: X().max, ticks: X().ticks, label: (v) => `${v}°` },
      y: { ...spec.axis, label: (v) => `${v}` },
      points: pts(),
      marker: temp != null && now != null ? { x: temp, y: now } : null,
      selected,
      editable: true,
    };
  }

  /** 当前曲线横轴对应的实时温度。 */
  function sourceTemp(): number | null {
    const st = store.status;
    return (source() === "gpu" ? st?.gpu_temp_c : st?.coolant_c) ?? null;
  }

  function drawChart() {
    renderChart(svg, chartOptions());
  }

  function drawTable() {
    const spec = SPEC[tab];
    const p = pts();
    root.querySelector(".th-value")!.textContent = `${spec.name} (${spec.unit})`;
    root.querySelector(".th-x")!.textContent = `${X().name} (°C)`;
    tbody.replaceChildren(
      ...p.map(([x, y], i) => {
        const xi = h("input", { type: "number", class: "num", min: X().min, max: X().max, step: X().step, value: x, "aria-label": `第 ${i + 1} 点${X().name}` });
        const yi = h("input", { type: "number", class: "num", min: spec.yMin, max: spec.yMax, step: spec.yStep, value: y, "aria-label": `第 ${i + 1} 点${spec.name}` });
        const del = h("button", { type: "button", class: "btn btn-small", disabled: p.length <= 2, "aria-label": `删除第 ${i + 1} 点` }, "删除");
        xi.addEventListener("change", () => editPoint(i, Number(xi.value), null));
        yi.addEventListener("change", () => editPoint(i, null, Number(yi.value)));
        del.addEventListener("click", () => removePoint(i));
        const tr = h("tr", { class: i === selected ? "selected" : "" }, h("td", { class: "mono" }, String(i + 1)), h("td", {}, xi), h("td", {}, yi), h("td", {}, del));
        tr.addEventListener("focusin", () => {
          if (selected !== i) {
            selected = i;
            drawChart();
            tbody.querySelectorAll("tr").forEach((r, j) => r.classList.toggle("selected", j === i));
          }
        });
        return tr;
      }),
    );
  }

  function drawControls() {
    if (!draft) return;
    const c = draft.cooling;
    modeButtons.forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.mode === c.mode)));
    tabButtons.forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.tab === tab)));
    fixedFan.value = String(c.fixed_fan_percent);
    fixedFanOut.textContent = `${c.fixed_fan_percent}%`;
    fixedPump.value = String(c.fixed_pump_rpm);
    fixedPumpOut.textContent = `${c.fixed_pump_rpm} rpm`;
    if (document.activeElement !== slew) slew.value = String(c.fan_slew_percent_per_s);
    addBtn.toggleAttribute("disabled", pts().length >= 12);
    sourceGroup.hidden = tab !== "fan";
    sourceButtons.forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.source === c.fan_source)));
    cpuBoost.value = String(c.cpu_boost_percent);
    cpuBoostOut.textContent = `+${c.cpu_boost_percent}%`;
    gpuBoost.value = String(c.gpu_boost_percent);
    gpuBoostOut.textContent = `+${c.gpu_boost_percent}%`;
    drawPerFan();
  }

  /** 单风扇覆盖：每个风扇一行，偏移量 + 可选固定值。焦点在某个输入框里时不重建，避免打断输入。 */
  function drawPerFan() {
    if (!draft || perFan.contains(document.activeElement)) return;
    const groups = store.status?.fan_groups ?? [];
    const overrides = draft.cooling.fan_overrides;
    const rows = groups.flatMap((g) =>
      g.rpm.map((_, i) => {
        const o = overrides.find((x) => x.address === g.address && x.index === i);
        const id = `${g.address}-${i}`;
        const offset = h("input", { type: "number", class: "num", id: `off-${id}`, min: -50, max: 50, step: 1, value: o?.offset_percent ?? 0 });
        const fixedOn = h("input", { type: "checkbox", id: `fix-${id}`, checked: o?.fixed_percent != null });
        const fixedVal = h("input", { type: "number", class: "num", min: 20, max: 100, step: 1, value: o?.fixed_percent ?? 50, disabled: o?.fixed_percent == null, "aria-label": `风扇 ${i + 1} 固定值` });
        const update = () => {
          if (!draft) return;
          const list = draft.cooling.fan_overrides.filter((x) => !(x.address === g.address && x.index === i));
          const off = clamp(Number(offset.value) || 0, -50, 50);
          const fixed = fixedOn.checked ? clamp(Number(fixedVal.value) || 50, 20, 100) : null;
          if (off !== 0 || fixed != null) list.push({ address: g.address, index: i, offset_percent: off, fixed_percent: fixed });
          draft.cooling.fan_overrides = list;
          fixedVal.toggleAttribute("disabled", !fixedOn.checked);
          drawStatus();
        };
        offset.addEventListener("change", update);
        fixedOn.addEventListener("change", update);
        fixedVal.addEventListener("change", update);
        return h(
          "div",
          { class: "per-fan-row" },
          h("span", {}, `${g.kind} ${i + 1}`),
          h("label", { for: `off-${id}`, class: "muted small" }, "偏移"),
          offset,
          h("label", { class: "check" }, fixedOn, "固定"),
          fixedVal,
        );
      }),
    );
    perFan.replaceChildren(...(rows.length ? rows : [h("p", { class: "muted small" }, "没有发现本机绑定的无线风扇")]));
  }

  function drawStatus() {
    const st = store.status;
    const c = draft?.cooling;
    const t = st?.coolant_c;
    const ft = c?.fan_source === "gpu" ? st?.gpu_temp_c : t;
    const fanTarget = c && ft != null ? (c.mode === "curve" ? evalCurve(c.fan_curve.points, ft) : c.fixed_fan_percent) : null;
    const pumpTarget = c && t != null ? (c.mode === "curve" ? evalCurve(c.pump_curve.points, t) : c.fixed_pump_rpm) : null;
    const rows: [string, string][] = [
      ["水温", `${fmt(t, 1)} °C`],
      ["GPU", `${fmt(st?.gpu_temp_c)} °C · ${fmt(st?.gpu_usage)}%`],
      ["CPU", `${fmt(st?.cpu_usage)}%`],
      ["风扇", `${fmt(st?.fan_percent)}% · 前馈 +${fmt(st?.boost_percent, 1)}`],
      ["泵", `${fmt(st?.pump_rpm)} rpm`],
      ["主板 PWM", st?.mb_pwm_percent != null ? `${fmt(st.mb_pwm_percent)}%` : "无信号"],
      ["草稿目标", `${fmt(fanTarget)}% · ${fmt(pumpTarget)} rpm`],
    ];
    readings.replaceChildren(...rows.flatMap(([k, v]) => [h("dt", {}, k), h("dd", { class: "mono" }, v)]));

    const isDirty = dirty();
    revertBtn.toggleAttribute("disabled", !isDirty);
    applyBtn.toggleAttribute("disabled", !isDirty);
    const preset = draft ? activePreset(draft) : null;
    statusText.textContent =
      message || (isDirty ? "有未应用的修改" : draft ? `已应用${preset ? ` · ${preset.name}预设` : ""}` : "等待后台服务…");
  }

  function redraw() {
    drawChart();
    drawTable();
    drawControls();
    drawStatus();
  }

  // ---------- 编辑 ----------
  /** 温度夹在相邻两点之间；数值也夹在相邻两点之间，保证曲线随温度不降（温度越高转速不能越低）。 */
  function constrain(i: number, x: number, y: number, p: Point[]): Point {
    const spec = SPEC[tab];
    const xLo = i > 0 ? p[i - 1][0] + X().gap : X().min;
    const xHi = i < p.length - 1 ? p[i + 1][0] - X().gap : X().max;
    const yLo = i > 0 ? p[i - 1][1] : spec.yMin;
    const yHi = i < p.length - 1 ? p[i + 1][1] : spec.yMax;
    return [clamp(snap(x, X().step), xLo, xHi), clamp(snap(y, spec.yStep), yLo, yHi)];
  }

  /** 排序后把数值改成不降：每个点至少等于前一个点。 */
  function monotone(p: Point[]) {
    for (let j = 1; j < p.length; j++) if (p[j][1] < p[j - 1][1]) p[j][1] = p[j - 1][1];
  }

  function editPoint(i: number, x: number | null, y: number | null) {
    const p = structuredClone(pts());
    if (!p[i]) return;
    if (x != null && Number.isFinite(x)) {
      // 表格里改温度可能越过相邻点：改完重新排序，选中跟着这个点走。
      p[i] = [clamp(snap(x, X().step), X().min, X().max), p[i][1]];
      const moved = p[i];
      p.sort((a, b) => a[0] - b[0]);
      i = p.indexOf(moved);
      for (let j = 1; j < p.length; j++) if (p[j][0] - p[j - 1][0] < X().gap) p[j][0] = p[j - 1][0] + X().gap;
      monotone(p);
    }
    if (y != null && Number.isFinite(y)) p[i] = constrain(i, p[i][0], y, p);
    selected = i;
    setPts(p);
    message = "";
    redraw();
  }

  function addPoint(x: number, y: number) {
    const p = structuredClone(pts());
    if (p.length >= 12) return;
    const sx = clamp(snap(x, X().step), X().min, X().max);
    if (p.some(([px]) => Math.abs(px - sx) < X().gap)) return;
    const np: Point = [sx, y];
    p.push(np);
    p.sort((a, b) => a[0] - b[0]);
    selected = p.indexOf(np);
    p[selected] = constrain(selected, sx, y, p);
    setPts(p);
    message = "";
    redraw();
  }

  function removePoint(i: number) {
    const p = structuredClone(pts());
    if (p.length <= 2) return;
    p.splice(i, 1);
    selected = null;
    setPts(p);
    message = "";
    redraw();
  }

  // 拖动：在 svg 上捕获指针，拖动中整张图重画但 svg 元素不变。
  svg.addEventListener("pointerdown", (e) => {
    const g = (e.target as Element).closest(".chart-point");
    if (!g) return;
    selected = Number(g.getAttribute("data-index"));
    dragging = selected;
    svg.setPointerCapture(e.pointerId);
    svg.focus();
    redraw();
  });
  svg.addEventListener("pointermove", (e) => {
    if (dragging == null) return;
    const { ix, iy } = scales(chartOptions());
    const { x, y } = toSvgPoint(svg, e);
    const p = structuredClone(pts());
    p[dragging] = constrain(dragging, ix(x), iy(y), p);
    setPts(p);
    drawChart();
    drawStatus();
  });
  const endDrag = () => {
    if (dragging == null) return;
    dragging = null;
    redraw();
  };
  svg.addEventListener("pointerup", endDrag);
  svg.addEventListener("pointercancel", endDrag);
  svg.addEventListener("dblclick", (e) => {
    if ((e.target as Element).closest(".chart-point")) return;
    const { ix, iy } = scales(chartOptions());
    const { x, y } = toSvgPoint(svg, e);
    addPoint(ix(x), iy(y));
  });
  svg.addEventListener("keydown", (e) => {
    if (selected == null) return;
    const spec = SPEC[tab];
    const [x, y] = pts()[selected] ?? [0, 0];
    const moves: Record<string, [number, number]> = {
      ArrowLeft: [-X().step, 0],
      ArrowRight: [X().step, 0],
      ArrowUp: [0, spec.yStep],
      ArrowDown: [0, -spec.yStep],
    };
    if (e.key in moves) {
      e.preventDefault();
      const [dx, dy] = moves[e.key];
      const k = e.shiftKey ? 10 : 1;
      const p = structuredClone(pts());
      p[selected] = constrain(selected, x + dx * k, y + dy * k, p);
      setPts(p);
      redraw();
    } else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      removePoint(selected);
    } else if (e.key === "Escape") {
      selected = null;
      redraw();
    }
  });

  addBtn.addEventListener("click", () => {
    const p = pts();
    if (p.length === 0) return addPoint(30, SPEC[tab].yMin);
    const i = selected ?? p.length - 1;
    const next = p[i + 1];
    // 加在选中点和下一个点中间；已经是最后一个就往右 5°C。
    addPoint(next ? (p[i][0] + next[0]) / 2 : p[i][0] + 5, next ? (p[i][1] + next[1]) / 2 : p[i][1]);
  });

  modeButtons.forEach((b) =>
    b.addEventListener("click", () => {
      if (!draft) return;
      draft.cooling.mode = b.dataset.mode as Mode;
      redraw();
    }),
  );
  tabButtons.forEach((b) =>
    b.addEventListener("click", () => {
      tab = b.dataset.tab as Tab;
      selected = null;
      redraw();
    }),
  );
  presetButtons.forEach((b, i) =>
    b.addEventListener("click", () => {
      if (!draft) return;
      draft = applyPreset(draft, PRESETS[i]);
      selected = null;
      redraw();
    }),
  );
  fixedFan.addEventListener("input", () => {
    if (!draft) return;
    draft.cooling.fixed_fan_percent = Number(fixedFan.value);
    drawControls();
    drawStatus();
  });
  fixedPump.addEventListener("input", () => {
    if (!draft) return;
    draft.cooling.fixed_pump_rpm = Number(fixedPump.value);
    drawControls();
    drawStatus();
  });
  sourceButtons.forEach((b) =>
    b.addEventListener("click", () => {
      if (!draft) return;
      const next = b.dataset.source as TempSource;
      if (next === draft.cooling.fan_source) return;
      // 两种温度的范围不同（水温 20–55、GPU 30–90），按比例换算横坐标，曲线形状不变。
      const [from, to] = next === "gpu" ? [X_COOLANT, X_GPU] : [X_GPU, X_COOLANT];
      const scale = (to.max - to.min) / (from.max - from.min);
      draft.cooling.fan_curve.points = draft.cooling.fan_curve.points.map(([x, y]): Point => [
        clamp(snap(to.min + (x - from.min) * scale, to.step), to.min, to.max),
        y,
      ]);
      draft.cooling.fan_source = next;
      selected = null;
      redraw();
    }),
  );
  for (const [input, key] of [
    [cpuBoost, "cpu_boost_percent"],
    [gpuBoost, "gpu_boost_percent"],
  ] as const) {
    input.addEventListener("input", () => {
      if (!draft) return;
      draft.cooling[key] = Number(input.value);
      drawControls();
      drawStatus();
    });
  }
  slew.addEventListener("change", () => {
    if (!draft) return;
    const v = Number(slew.value);
    if (Number.isFinite(v)) draft.cooling.fan_slew_percent_per_s = clamp(v, 0.5, 100);
    drawControls();
    drawStatus();
  });
  revertBtn.addEventListener("click", () => {
    draft = base ? structuredClone(base) : null;
    selected = null;
    message = "";
    redraw();
  });
  applyBtn.addEventListener("click", async () => {
    if (!draft) return;
    applyBtn.setAttribute("disabled", "");
    try {
      await saveConfig(draft);
      // 以后台规范化后的版本为准（比如越界值被截断）。
      reset();
      message = "";
    } catch (e) {
      message = `应用失败：${e}`;
    }
    redraw();
  });

  function update() {
    // 后台配置变了（比如在概览页切了预设）：没有未应用的修改就跟着更新，有修改则保留草稿。
    if (store.config && !same(base, store.config)) {
      if (dirty()) {
        base = structuredClone(store.config);
      } else if (dragging == null) {
        reset();
        selected = null;
        drawTable();
        drawControls();
      }
    }
    if (dragging == null) drawChart();
    drawPerFan();
    drawStatus();
  }

  // 第一次进入就画一次，避免等下一秒轮询。
  reset();
  redraw();

  return { root, update };
}
