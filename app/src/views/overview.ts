import { renderChart } from "../chart";
import { activePreset, applyPreset, PRESETS } from "../curve";
import { fmt, h, s, sparkline } from "../dom";
import { saveConfig, store } from "../store";
import type { View } from "./view";

const COLORS = { coolant: "#F2A33A", pump: "#6FB3E0", fan: "#5BC08A", cpu: "#B9A7E8", gpu: "#E58FB6" };

function tile(label: string, unit: string) {
  const value = h("span", { class: "tile-value" }, "—");
  const caption = h("span", { class: "tile-label" }, label);
  const spark = h("div", { class: "tile-spark" });
  const el = h(
    "div",
    { class: "tile" },
    caption,
    h("div", { class: "tile-reading" }, value, h("span", { class: "tile-unit" }, unit)),
    spark,
  );
  return { el, value, caption, spark };
}

export function overview(): View {
  const subtitle = h("span", { class: "subtitle" });
  const presetButtons = PRESETS.map((p) =>
    h("button", { type: "button", class: "btn", "data-preset": p.id, "aria-pressed": "false" }, p.name),
  );
  const coolant = tile("水温", "°C");
  const pump = tile("泵速", "rpm");
  const fan = tile("风扇占空比", "%");
  const cpu = tile("CPU 占用", "%");
  const gpu = tile("GPU 温度", "°C");
  const fans = h("div", { class: "fan-list" });
  const fanTitle = h("h2", {}, "风扇");
  const fanAddr = h("span", { class: "mono muted small" });
  const curveSvg = s("svg", { class: "chart", role: "img", "aria-label": "当前风扇曲线与工作点" });
  const curveTitle = h("h2", {}, "当前曲线 · 水温 → 风扇");
  const devices = h("tbody");

  const root = h(
    "div",
    { class: "page" },
    h(
      "header",
      { class: "page-head" },
      h("div", { class: "stack-tight" }, h("h1", {}, "概览"), subtitle),
      h("div", { class: "btn-row", role: "group", "aria-label": "预设档位" }, ...presetButtons),
    ),
    h("section", { class: "tiles", "aria-label": "实时读数" }, coolant.el, pump.el, fan.el, cpu.el, gpu.el),
    h(
      "section",
      { class: "split" },
      h("div", { class: "card card-fans" }, h("div", { class: "card-head" }, fanTitle, fanAddr), fans),
      h(
        "div",
        { class: "card card-curve" },
        h("div", { class: "card-head" }, curveTitle, h("a", { href: "#/curves" }, "编辑")),
        curveSvg,
      ),
    ),
    h(
      "section",
      { class: "card card-table", "aria-label": "设备" },
      h(
        "div",
        { class: "table-wrap" },
        h(
          "table",
          {},
          h("thead", {}, h("tr", {}, h("th", {}, "设备"), h("th", {}, "连接"), h("th", {}, "地址"))),
          devices,
        ),
      ),
    ),
  );

  presetButtons.forEach((btn, i) => {
    btn.addEventListener("click", () => {
      if (store.config) saveConfig(applyPreset(store.config, PRESETS[i]));
    });
  });

  function update() {
    const st = store.status;
    const cfg = store.config;
    const hist = store.history;

    const preset = cfg ? activePreset(cfg) : null;
    const modeText = !cfg
      ? ""
      : cfg.cooling.mode === "fixed"
        ? "固定转速"
        : cfg.cooling.mode === "motherboard"
          ? "跟随主板"
          : preset
            ? `${preset.name}曲线`
            : cfg.cooling.fan_source === "gpu"
              ? "自定义曲线（GPU 温度）"
              : "自定义曲线";
    const names = st?.devices.filter((d) => !d.name.startsWith("L-Wireless")).map((d) => d.name) ?? [];
    subtitle.textContent = [...names, modeText && `模式：${modeText}`].filter(Boolean).join(" · ");
    presetButtons.forEach((b) => b.setAttribute("aria-pressed", String(preset?.id === b.dataset.preset)));

    coolant.value.textContent = fmt(st?.coolant_c, 1);
    coolant.spark.replaceChildren(sparkline(hist.coolant, COLORS.coolant, 1));
    pump.value.textContent = fmt(st?.pump_rpm);
    pump.caption.textContent = st?.pump_target_rpm != null ? `泵速 · 目标 ${st.pump_target_rpm}` : "泵速";
    pump.spark.replaceChildren(sparkline(hist.pump, COLORS.pump, 100));
    fan.value.textContent = fmt(st?.fan_percent);
    fan.caption.textContent = st?.boost_percent ? `风扇占空比 · 前馈 +${fmt(st.boost_percent, 1)}` : "风扇占空比";
    fan.spark.replaceChildren(sparkline(hist.fan, COLORS.fan, 5));
    cpu.value.textContent = fmt(st?.cpu_usage);
    cpu.spark.replaceChildren(sparkline(hist.cpu, COLORS.cpu, 10));
    gpu.value.textContent = fmt(st?.gpu_temp_c);
    gpu.caption.textContent = st?.gpu_usage != null ? `GPU 温度 · 占用 ${fmt(st.gpu_usage)}% · ${fmt(st.gpu_power_w)} W` : "GPU 温度";
    gpu.spark.replaceChildren(sparkline(hist.gpu, COLORS.gpu, 2));

    const groups = st?.fan_groups ?? [];
    fanTitle.textContent = groups.length ? groups.map((g) => `${g.kind} × ${g.rpm.length}`).join(" · ") : "风扇";
    fanAddr.textContent = groups.map((g) => g.address).join(" · ");
    fans.replaceChildren(
      ...(groups.length
        ? groups.flatMap((g) =>
            g.rpm.map((rpm, i) => {
              const pwm = g.pwm[i] ?? 0;
              const fill = h("div", { class: "bar-fill" });
              fill.style.width = `${Math.round((pwm / 255) * 100)}%`;
              return h(
                "div",
                { class: "fan-row" },
                h("div", { class: "fan-row-head" }, h("span", {}, `风扇 ${i + 1}`), h("span", { class: "mono" }, `${rpm} rpm · PWM ${pwm}`)),
                h("div", { class: "bar" }, fill),
              );
            }),
          )
        : [h("p", { class: "muted" }, "没有发现绑定到本机的无线风扇")]),
    );

    if (cfg) {
      const c = cfg.cooling;
      const gpuSrc = c.fan_source === "gpu";
      const temp = gpuSrc ? st?.gpu_temp_c : st?.coolant_c;
      const fanNow = temp != null && st?.fan_percent != null ? { x: temp, y: st.fan_percent } : null;
      const [lo, hi] = gpuSrc ? [30, 90] : [20, 50];
      const flat = (v: number): [number, number][] => [[lo, v], [hi, v]];
      renderChart(curveSvg, {
        width: 440,
        height: 200,
        x: { min: lo, max: hi, ticks: gpuSrc ? [30, 50, 70, 90] : [20, 30, 40, 50], label: (v) => `${v}°` },
        y: { min: 0, max: 100, ticks: [0, 20, 60, 100], label: (v) => `${v}` },
        points: c.mode === "curve" ? c.fan_curve.points : flat(c.mode === "motherboard" ? st?.mb_pwm_percent ?? c.fixed_fan_percent : c.fixed_fan_percent),
        marker: fanNow,
      });
      curveTitle.textContent =
        c.mode === "curve" ? `当前曲线 · ${gpuSrc ? "GPU 温度" : "水温"} → 风扇` : c.mode === "motherboard" ? "跟随主板 PWM" : "固定转速";
    }

    devices.replaceChildren(
      ...(st?.devices ?? []).map((d) =>
        h("tr", {}, h("td", {}, d.name), h("td", {}, d.connection), h("td", { class: "mono" }, d.address)),
      ),
    );
  }

  return { root, update };
}
