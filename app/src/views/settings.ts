import { api } from "../api";
import { h } from "../dom";
import { store } from "../store";
import type { View } from "./view";

export function settings(): View {
  const autostart = h("input", { type: "checkbox", id: "autostart" });
  const autostartNote = h("span", { class: "muted small" });
  const configPath = h("code", { class: "mono small path" }, "…");
  const logPath = h("code", { class: "mono small path" }, "…");
  const exportBtn = h("button", { type: "button", class: "btn" }, "导出配置…");
  const importBtn = h("button", { type: "button", class: "btn" }, "导入配置…");
  const configMsg = h("p", { class: "muted small", role: "status" });
  const serviceText = h("span", {});
  const startBtn = h("button", { type: "button", class: "btn btn-primary" }, "启动后台服务");

  const root = h(
    "div",
    { class: "page" },
    h("header", { class: "page-head" }, h("h1", {}, "设置")),
    h(
      "section",
      { class: "card" },
      h("h2", {}, "后台服务"),
      h("div", { class: "row-between" }, serviceText, startBtn),
      h("div", { class: "row-between" }, h("label", { class: "check", for: "autostart" }, autostart, "登录 Windows 时自动启动后台服务"), autostartNote),
      h("p", { class: "hint" }, "后台服务在托盘里常驻，右键托盘图标可以切换预设或退出；关闭这个窗口不影响风扇控制。"),
    ),
    h(
      "section",
      { class: "card" },
      h("h2", {}, "配置文件"),
      h("div", { class: "kv" }, h("span", { class: "muted" }, "位置"), configPath),
      h("div", { class: "btn-row" }, exportBtn, importBtn),
      configMsg,
    ),
    h("section", { class: "card" }, h("h2", {}, "日志"), h("div", { class: "kv" }, h("span", { class: "muted" }, "位置"), logPath)),
    h(
      "section",
      { class: "card" },
      h("h2", {}, "关于"),
      h("p", { class: "small" }, "LConnect Lite 0.1.0 · 联力 L-Connect 3 的轻量替代。"),
      h("p", { class: "small muted" }, "协议实现移植自 lian-li-linux（MIT）。与联力官方无关。"),
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
      autostartNote.textContent = "";
    } catch {
      autostart.disabled = true;
      autostartNote.textContent = "需要后台服务运行";
    }
  }

  autostart.addEventListener("change", async () => {
    try {
      await api.setAutostart(autostart.checked);
    } catch (e) {
      autostartNote.textContent = String(e);
    }
    refreshAutostart();
  });
  exportBtn.addEventListener("click", async () => {
    try {
      const path = await api.exportConfig();
      configMsg.textContent = path ? `已导出到 ${path}` : "";
    } catch (e) {
      configMsg.textContent = `导出失败：${e}`;
    }
  });
  importBtn.addEventListener("click", async () => {
    try {
      const path = await api.importConfig();
      if (path) {
        store.config = null; // 下一次轮询重新读取
        configMsg.textContent = `已从 ${path} 导入并应用`;
      }
    } catch (e) {
      configMsg.textContent = `导入失败：${e}`;
    }
  });
  startBtn.addEventListener("click", async () => {
    try {
      await api.startDaemon();
      serviceText.textContent = "正在启动…";
    } catch (e) {
      serviceText.textContent = String(e);
    }
  });

  let wasOffline: boolean | null = null;
  function update() {
    const offline = store.offline != null;
    serviceText.textContent = offline ? "未运行" : `运行中 · 已运行 ${store.status?.uptime_s ?? 0} 秒`;
    startBtn.hidden = !offline;
    if (offline !== wasOffline) {
      wasOffline = offline;
      refreshAutostart();
    }
  }

  return { root, update };
}
