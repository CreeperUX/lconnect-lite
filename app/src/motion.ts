/**
 * 动效小工具。原则：只在值真正变化时动；尊重系统的“减少动态效果”；
 * 尽量只动 transform / opacity。时长、曲线和 style.css 里的 --dur-* / --ease-out 对应。
 */

const media = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;
export const reducedMotion = () => !!media?.matches;

export const EASE_OUT = "cubic-bezier(0.2, 0.8, 0.2, 1)";
export const DUR = { fast: 120, base: 180, slow: 240, /** 读数滚动：比界面动效稍长，一秒一次的数据看起来更顺 */ data: 450 };

// ---------- 离场 ----------

/**
 * 加上离场 class，等它的动画播完（或超时兜底）再执行 `done`。
 * 返回一个取消函数：撤掉 class，不再执行 `done`。
 */
export function playOut(el: Element, cls: string, done: () => void, timeout = 400): () => void {
  let finished = false;
  let timer = 0;
  const onEnd = (e: Event) => {
    if (e.target === el) finish();
  };
  const stop = () => {
    finished = true;
    el.removeEventListener("animationend", onEnd);
    clearTimeout(timer);
  };
  function finish() {
    if (finished) return;
    stop();
    done();
  }
  el.classList.add(cls);
  if (reducedMotion() || !el.isConnected || getComputedStyle(el).animationName === "none") {
    finish();
    return () => {};
  }
  el.addEventListener("animationend", onEnd);
  timer = window.setTimeout(finish, timeout);
  return () => {
    if (finished) return;
    stop();
    el.classList.remove(cls);
  };
}

const exits = new WeakMap<Element, () => void>();

/** 显示（会打断正在播的离场动画）。入场动画由 CSS 负责：display 从 none 变回来时自动播放。 */
export function reveal(el: HTMLElement) {
  exits.get(el)?.();
  exits.delete(el);
  el.hidden = false;
}

/** 播完离场动画再隐藏。 */
export function conceal(el: HTMLElement, cls = "is-leaving") {
  if (el.hidden || exits.has(el)) return;
  const cancel = playOut(el, cls, () => {
    exits.delete(el);
    el.classList.remove(cls);
    el.hidden = true;
  });
  if (!el.hidden) exits.set(el, cancel);
}

// ---------- 折叠 ----------

const folds = new WeakMap<HTMLElement, Animation>();

/** 展开 / 收起：高度和透明度一起过渡；连点时从当前位置接着动。 */
export function fold(el: HTMLElement, open: boolean) {
  const prev = folds.get(el);
  const from = el.hidden ? 0 : el.getBoundingClientRect().height;
  const fromOpacity = prev ? Number(getComputedStyle(el).opacity) : open ? 0 : 1;
  prev?.cancel();
  folds.delete(el);
  el.hidden = false;
  if (reducedMotion() || typeof el.animate !== "function") {
    el.hidden = !open;
    return;
  }
  const to = open ? el.getBoundingClientRect().height : 0;
  el.style.overflow = "hidden";
  const anim = el.animate(
    [
      { height: `${from}px`, opacity: fromOpacity },
      { height: `${to}px`, opacity: open ? 1 : 0 },
    ],
    { duration: DUR.slow, easing: EASE_OUT },
  );
  folds.set(el, anim);
  const clear = () => {
    if (folds.get(el) !== anim) return;
    folds.delete(el);
    el.style.overflow = "";
  };
  anim.addEventListener("finish", () => {
    if (folds.get(el) !== anim) return;
    if (!open) el.hidden = true;
    clear();
  });
  anim.addEventListener("cancel", clear);
}

// ---------- 读数滚动 ----------

type Run = { from: number; to: number; t0: number; cur: number; format: (v: number) => string };
const runs = new Map<Element, Run>();
const shown = new WeakMap<Element, number>();
let raf = 0;

/**
 * 把元素里的数字平滑地滚到 `v`。每秒用同一个值调用不会重播；
 * 第一次显示、从“—”变成数字、页面在后台时直接跳到新值。
 */
export function countTo(el: Element, v: number | null | undefined, format: (v: number) => string, empty = "—") {
  if (v == null || !Number.isFinite(v)) {
    runs.delete(el);
    shown.delete(el);
    if (el.textContent !== empty) el.textContent = empty;
    return;
  }
  const run = runs.get(el);
  if (run) {
    run.format = format;
    if (run.to === v) return;
  } else if (shown.get(el) === v) {
    const text = format(v);
    if (el.textContent !== text) el.textContent = text;
    return;
  }
  const from = run ? run.cur : shown.get(el);
  if (from == null || reducedMotion() || document.hidden || format(from) === format(v)) {
    runs.delete(el);
    shown.set(el, v);
    const text = format(v);
    if (el.textContent !== text) el.textContent = text;
    return;
  }
  runs.set(el, { from, to: v, t0: performance.now(), cur: from, format });
  if (!raf) raf = requestAnimationFrame(tick);
}

function tick(now: number) {
  raf = 0;
  for (const [el, r] of runs) {
    const p = Math.min(1, Math.max(0, (now - r.t0) / DUR.data));
    const k = 1 - Math.pow(1 - p, 3);
    r.cur = p >= 1 ? r.to : r.from + (r.to - r.from) * k;
    const text = r.format(r.cur);
    if (el.textContent !== text) el.textContent = text;
    if (p >= 1) {
      runs.delete(el);
      shown.set(el, r.to);
    }
  }
  if (runs.size) raf = requestAnimationFrame(tick);
}
