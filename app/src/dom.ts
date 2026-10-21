type Attrs = Record<string, string | number | boolean | null | undefined>;
type Child = Node | string | null | undefined | false;

function apply(el: Element, attrs: Attrs, children: Child[]) {
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    el.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of children) {
    if (c == null || c === false) continue;
    el.append(c);
  }
  return el;
}

/** 建一个 HTML 元素：h("div", { class: "x" }, "文字", child)。 */
export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attrs = {}, ...children: Child[]) {
  return apply(document.createElement(tag), attrs, children) as HTMLElementTagNameMap[K];
}

const SVG_NS = "http://www.w3.org/2000/svg";

/** 建一个 SVG 元素。 */
export function s<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Attrs = {}, ...children: Child[]) {
  return apply(document.createElementNS(SVG_NS, tag), attrs, children) as SVGElementTagNameMap[K];
}

export function fmt(v: number | null | undefined, digits = 0): string {
  return v == null || Number.isNaN(v) ? "—" : v.toFixed(digits);
}

/** 迷你走势图：把一串数值画成折线，纵轴按数据范围自动缩放。 */
export function sparkline(values: number[], color: string, minSpan = 1): SVGSVGElement {
  const W = 200;
  const H = 40;
  const svg = s("svg", { viewBox: `0 0 ${W} ${H}`, width: "100%", height: H, preserveAspectRatio: "none", "aria-hidden": "true" });
  if (values.length < 2) return svg;
  let lo = Math.min(...values);
  let hi = Math.max(...values);
  if (hi - lo < minSpan) {
    const mid = (hi + lo) / 2;
    lo = mid - minSpan / 2;
    hi = mid + minSpan / 2;
  }
  const pts = values
    .map((v, i) => `${((i / (values.length - 1)) * W).toFixed(1)},${(H - 4 - ((v - lo) / (hi - lo)) * (H - 8)).toFixed(1)}`)
    .join(" ");
  svg.append(s("polyline", { points: pts, fill: "none", stroke: color, "stroke-width": 2, "vector-effect": "non-scaling-stroke" }));
  return svg;
}
