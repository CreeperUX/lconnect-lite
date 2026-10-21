import { api } from "../api";
import { h } from "../dom";
import { icon } from "../icons";
import { store } from "../store";
import { row, toast, toggle } from "../ui";
import type { View } from "./view";

export function settings(): View {
  const serviceText = h("span", { class: "row-desc" });
  const startBtn = h("button", { type: "button", class: "btn btn-sm btn-primary reveal" }, icon("power"), "启动");
  const autostart = toggle("登录时自动启动", async (on) => {
    try {
      await api.setAutostart(on);
      toast(on ? "已设为登录时自动启动" : "已取消自动启动", { key: "autostart" });
    } catch (e) {
      toast(`设置失败：${e}`, { error: true, key: "autostart" });
    }
    refreshAutostart();
  });
  const configPath = h("span", { class: "mono small", style: "color: var(--text-2); word-break: break-all" }, "…");
  const logPath = h("span", { class: "mono small", style: "color: var(--text-2); word-break: break-all" }, "…");
  const exportBtn = h("button", { type: "button", class: "btn btn-sm" }, "导出…");
  const importBtn = h("button", { type: "button", class: "btn btn-sm" }, "导入…");
  const revealBtn = (which: "config" | "log") => {
    const b = h("button", { type: "button", class: "btn btn-sm" }, "在文件夹中显示");
    b.addEventListener("click", () => api.revealPath(which).catch((e) => toast(`打不开：${e}`, { error: true, key: "reveal" })));
    return b;
  };

  const group = (title: string, ...rows: HTMLElement[]) =>
    h("section", { style: "display: flex; flex-direction: column; gap: 12px" }, h("h2", {}, title), h("div", { class: "list" }, ...rows));

  const root = h(
    "div",
    { class: "page" },
    h("header", { class: "page-head" }, h("div", { class: "page-title" }, h("h1", {}, "设置"), h("span", { class: "subtitle" }, "关闭这个窗口不影响风扇控制：后台服务在托盘里常驻"))),
    group(
      "后台服务",
      row("状态", serviceText, startBtn),
      row("登录 Windows 时自动启动", "后台服务随登录启动，常驻托盘；右键托盘图标可以切换档位或退出", autostart),
    ),
    group(
      "配置",
      row("配置文件", configPath, revealBtn("config"), exportBtn, importBtn),
      row("日志", logPath, revealBtn("log")),
    ),
    group(
      "关于",
      row("LConnect Lite 0.2", "联力 L-Connect 3 的轻量替代，支持隐流二代 LCD-S 和 UNI FAN TL 无线。与联力官方无关"),
      row("开源组件", "协议实现移植自 lian-li-linux（MIT）；压缩用 tinyuz / HDiffPatch（MIT）"),
    ),
  );

  api.paths().then((p) => {
    configPath.textContent = p.config;
    logPath.textContent = p.log;
  });

  async function refreshAutostart() {
    try {
      autostart.checked = await api.getAutostart();
      autostart.disabled = false;
    } catch {
      autostart.disabled = true;
    }
  }

  exportBtn.addEventListener("click", async () => {
    try {
      const path = await api.exportConfig();
      if (path) toast(`已导出到 ${path}`, { key: "config-io" });
    } catch (e) {
      toast(`导出失败：${e}`, { error: true, key: "config-io" });
    }
  });
  importBtn.addEventListener("click", async () => {
    try {
      const path = await api.importConfig();
      if (path) {
        store.config = null; // 下一次轮询重新读取
        toast(`已从 ${path} 导入并应用`, { key: "config-io" });
      }
    } catch (e) {
      toast(`导入失败：${e}`, { error: true, key: "config-io" });
    }
  });
  startBtn.addEventListener("click", async () => {
    try {
      await api.startDaemon();
      serviceText.textContent = "正在启动…";
    } catch (e) {
      toast(String(e), { error: true, key: "start" });
    }
  });

  let wasOffline: boolean | null = null;
  function update() {
    const offline = store.offline != null;
    const up = store.status?.uptime_s ?? 0;
    const human = up >= 3600 ? `${Math.floor(up / 3600)} 小时 ${Math.floor((up % 3600) / 60)} 分` : up >= 60 ? `${Math.floor(up / 60)} 分钟` : `${up} 秒`;
    serviceText.textContent = offline ? "没有运行" : `运行中 · 已运行 ${human}${store.status?.memory_mb != null ? ` · 内存 ${store.status.memory_mb.toFixed(1)} MB` : ""}`;
    startBtn.hidden = !offline;
    if (offline !== wasOffline) {
      wasOffline = offline;
      refreshAutostart();
    }
  }

  return { root, update };
}
