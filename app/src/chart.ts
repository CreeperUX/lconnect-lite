import type { Point } from "./api";
import { s } from "./dom";

export interface Axis {
  min: number;
  max: number;
  ticks: readonly number[];
  label: (v: number) => string;
}

export interface ChartOptions {
  width: number;
  height: number;
  x: Axis;
  y: Axis;
  points: Point[];
  /** 当前工作点（水温, 输出）。 */
  marker?: { x: number; y: number } | null;
  selected?: number | null;
  /** 可编辑时点画得更大，并带透明的大命中区。 */
  editable?: boolean;
}

const MARGIN = { l: 48, r: 18, t: 14, b: 30 };
let clipSeq = 0;

export function scales(o: ChartOptions) {
  const w = o.width - MARGIN.l - MARGIN.r;
  const h = o.height - MARGIN.t - MARGIN.b;
  const sx = (v: number) => MARGIN.l + ((v - o.x.min) / (o.x.max - o.x.min)) * w;
  const sy = (v: number) => MARGIN.t + h - ((v - o.y.min) / (o.y.max - o.y.min)) * h;
  const ix = (px: number) => o.x.min + ((px - MARGIN.l) / w) * (o.x.max - o.x.min);
  const iy = (py: number) => o.y.min + ((MARGIN.t + h - py) / h) * (o.y.max - o.y.min);
  return { sx, sy, ix, iy, left: MARGIN.l, right: o.width - MARGIN.r, top: MARGIN.t, bottom: MARGIN.t + h };
}

/** 把曲线画进 `svg`（会先清空）。拖动时整张图重画，svg 元素本身保持不变。 */
export function renderChart(svg: SVGSVGElement, o: ChartOptions) {
  const { sx, sy, left, right, top, bottom } = scales(o);
  svg.setAttribute("viewBox", `0 0 ${o.width} ${o.height}`);
  svg.replaceChildren();

  const grid = s("g", { class: "chart-grid" });
  for (const t of o.y.ticks) {
    grid.append(s("line", { x1: left, x2: right, y1: sy(t), y2: sy(t) }));
  }
  svg.append(grid);

  const labels = s("g", { class: "chart-label" });
  for (const t of o.y.ticks) {
    labels.append(s("text", { x: left - 8, y: sy(t) + 4, "text-anchor": "end" }, o.y.label(t)));
  }
  for (const t of o.x.ticks) {
    labels.append(s("text", { x: sx(t), y: bottom + 20, "text-anchor": "middle" }, o.x.label(t)));
  }
  svg.append(labels);

  // 曲线只画在绘图区内；点落在坐标范围外时也不会画到轴标签上。
  const clipId = `chart-clip-${++clipSeq}`;
  svg.append(
    s("defs", {}, s("clipPath", { id: clipId }, s("rect", { x: left, y: top - 8, width: right - left, height: bottom - top + 16 }))),
  );
  const plot = s("g", { "clip-path": `url(#${clipId})` });
  svg.append(plot);

  const pts = o.points;
  if (pts.length > 0) {
    const first = pts[0];
    const last = pts[pts.length - 1];
    // 曲线两端之外取端点值，画成虚线表示“延伸”。
    plot.append(
      s("polyline", {
        class: "chart-extend",
        points: `${left},${sy(first[1])} ${sx(first[0])},${sy(first[1])}`,
      }),
      s("polyline", {
        class: "chart-extend",
        points: `${sx(last[0])},${sy(last[1])} ${right},${sy(last[1])}`,
      }),
      s("polyline", { class: "chart-line", points: pts.map(([x, y]) => `${sx(x)},${sy(y)}`).join(" ") }),
    );
  }

  if (o.marker) {
    const mx = Math.min(Math.max(sx(o.marker.x), left), right);
    svg.append(
      s("line", { class: "chart-now", x1: mx, x2: mx, y1: top, y2: bottom }),
      s("circle", { class: "chart-marker", cx: mx, cy: sy(o.marker.y), r: 6 }),
    );
  }

  pts.forEach(([x, y], i) => {
    const g = s("g", { class: i === o.selected ? "chart-point selected" : "chart-point", "data-index": i });
    if (o.editable) g.append(s("circle", { class: "chart-hit", cx: sx(x), cy: sy(y), r: 16 }));
    g.append(s("circle", { cx: sx(x), cy: sy(y), r: o.editable ? 7 : 4 }));
    svg.append(g);
  });
}

/** 把指针事件的屏幕坐标换成 svg viewBox 坐标。 */
export function toSvgPoint(svg: SVGSVGElement, e: PointerEvent | MouseEvent) {
  const ctm = svg.getScreenCTM();
  if (!ctm) return { x: 0, y: 0 };
  const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
  return { x: p.x, y: p.y };
}
