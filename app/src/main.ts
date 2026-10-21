import { api, isMock } from "./api";
import { startPolling, store, subscribe } from "./store";
import { curves } from "./views/curves";
import { devices } from "./views/devices";
import { lighting } from "./views/lighting";
import { overview } from "./views/overview";
import { screen } from "./views/screen";
import { settings } from "./views/settings";
import type { View } from "./views/view";

const routes: Record<string, () => View> = { overview, curves, lighting, screen, devices, settings };

const viewRoot = document.getElementById("view")!;
let current: View | null = null;

function navigate() {
  const name = location.hash.replace(/^#\//, "") || "overview";
  const make = routes[name] ?? routes.overview;
  current?.dispose?.();
  current = make();
  viewRoot.replaceChildren(current.root);
  current.update();
  document.querySelectorAll<HTMLAnchorElement>(".nav-item[data-route]").forEach((a) => {
    const active = a.dataset.route === name;
    a.classList.toggle("active", active);
    if (active) a.setAttribute("aria-current", "page");
    else a.removeAttribute("aria-current");
  });
}

function updateChrome() {
  const dot = document.getElementById("service-dot")!;
  const text = document.getElementById("service-text")!;
  const tx = document.getElementById("service-tx")!;
  const banner = document.getElementById("banner")!;
  const st = store.status;

  if (store.offline) {
    dot.className = "dot dot-bad";
    text.textContent = "后台服务未连接";
    tx.textContent = "";
    banner.hidden = false;
    // 每秒都会进来；内容没变就不重建，免得按钮在点击时被替换。
    if (banner.dataset.offline === store.offline) return;
    banner.dataset.offline = store.offline;
    const start = document.createElement("button");
    start.type = "button";
    start.className = "btn btn-small";
    start.textContent = "启动后台服务";
    start.addEventListener("click", () => api.startDaemon().catch((e) => (banner.textContent = String(e))));
    const msg = document.createElement("span");
    msg.textContent = store.offline;
    const row = document.createElement("div");
    row.className = "banner-row";
    row.append(msg, start);
    banner.replaceChildren(row);
    return;
  }
  delete banner.dataset.offline;
  dot.className = "dot dot-ok";
  text.textContent = isMock ? "模拟数据（浏览器预览）" : `后台服务运行中 · ${st?.uptime_s ?? 0}s`;
  const radio = st?.devices.find((d) => d.name.startsWith("L-Wireless"));
  tx.textContent = radio ? `TX ${radio.address} · ${radio.connection.replace("USB · ", "")}` : "";
  banner.hidden = !st?.last_error;
  banner.textContent = st?.last_error ?? "";
}

subscribe(() => {
  updateChrome();
  current?.update();
});
window.addEventListener("hashchange", navigate);
navigate();
startPolling();
