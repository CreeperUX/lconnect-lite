import { api, type WirelessDevice } from "../api";
import { h } from "../dom";
import { saveConfig, store } from "../store";
import type { View } from "./view";

const OWNER: Record<WirelessDevice["owner"], string> = { ours: "本机", unbound: "未绑定", other: "其他发射器" };
/** 解绑要在这段时间内再点一次才执行，防止误触。 */
const CONFIRM_MS = 5000;

export function devices(): View {
  const txText = h("span", { class: "mono" });
  const usbBody = h("tbody");
  const wlBody = h("tbody");
  const lockToggle = h("input", { type: "checkbox", id: "lock" });
  const message = h("p", { class: "small", role: "status" });
  let busy: string | null = null;
  let confirming: { address: string; until: number } | null = null;
  let lastKey = "";

  const root = h(
    "div",
    { class: "page" },
    h(
      "header",
      { class: "page-head" },
      h("div", { class: "stack-tight" }, h("h1", {}, "设备与绑定"), h("span", { class: "subtitle" }, "绑定和解绑会等设备确认后才保存；L-Connect 同时运行时两边会互相抢绑定。")),
    ),
    h(
      "section",
      { class: "card" },
      h("div", { class: "card-head" }, h("h2", {}, "无线设备"), h("span", { class: "small muted" }, "发射器 ", txText)),
      h(
        "div",
        { class: "table-wrap" },
        h(
          "table",
          {},
          h(
            "thead",
            {},
            h("tr", {}, h("th", {}, "设备"), h("th", {}, "地址"), h("th", {}, "归属"), h("th", {}, "rx / 频道"), h("th", {}, "主板灯光同步"), h("th", { class: "th-lock" }, "受控"), h("th", {}, "")),
          ),
          wlBody,
        ),
      ),
      h("label", { class: "check", for: "lock" }, lockToggle, "锁定绑定：只控制勾选“受控”的设备（只在电脑这边生效，不向设备发命令）"),
      message,
    ),
    h(
      "section",
      { class: "card" },
      h("h2", {}, "USB 设备"),
      h("div", { class: "table-wrap" }, h("table", {}, h("thead", {}, h("tr", {}, h("th", {}, "设备"), h("th", {}, "连接"), h("th", {}, "地址"))), usbBody)),
    ),
    h("p", { class: "hint" }, "切换无线频道暂不支持：漏收切换命令的设备会留在旧频道失联，需要时请用 L-Connect 操作。"),
  );

  async function run(label: string, address: string, op: () => Promise<void>) {
    busy = address;
    message.className = "small";
    message.textContent = `${label}中…（最多 15 秒）`;
    lastKey = "";
    draw();
    try {
      await op();
      message.textContent = `${label}成功`;
    } catch (e) {
      message.className = "small error-text";
      message.textContent = `${label}失败：${e}`;
    } finally {
      busy = null;
      lastKey = "";
      draw();
    }
  }

  async function setLocked(list: string[]) {
    if (!store.config) return;
    const next = structuredClone(store.config);
    next.devices.locked = list;
    try {
      await saveConfig(next);
    } catch (e) {
      message.textContent = `保存失败：${e}`;
    }
    lastKey = "";
    draw();
  }

  lockToggle.addEventListener("change", () => {
    const ours = (store.status?.wireless ?? []).filter((w) => w.owner === "ours").map((w) => w.address);
    // 打开锁定时默认锁定当前所有本机设备；关闭即清空列表。
    setLocked(lockToggle.checked ? ours : []);
  });

  function draw() {
    const st = store.status;
    const locked = store.config?.devices.locked ?? [];
    const key = JSON.stringify([st?.wireless, st?.devices, locked, busy, confirming]);
    if (key === lastKey) return; // 内容没变就不重建表格，免得打断点击
    lastKey = key;

    txText.textContent = st?.transmitter ? `${st.transmitter[0]} · 频道 ${st.transmitter[1]}` : "未连接";
    lockToggle.checked = locked.length > 0;
    root.querySelector(".th-lock")!.toggleAttribute("hidden", locked.length === 0);

    wlBody.replaceChildren(
      ...(st?.wireless ?? []).map((w) => {
        const ours = w.owner === "ours";
        const isBusy = busy === w.address;
        let action: HTMLElement;
        if (ours) {
          const armed = confirming?.address === w.address && Date.now() < confirming.until;
          action = h("button", { type: "button", class: armed ? "btn btn-small btn-danger" : "btn btn-small", disabled: busy != null }, isBusy ? "解绑中…" : armed ? "确认解绑" : "解绑");
          action.addEventListener("click", () => {
            if (armed) {
              confirming = null;
              run("解绑", w.address, () => api.unbind(w.address));
            } else {
              confirming = { address: w.address, until: Date.now() + CONFIRM_MS };
              lastKey = "";
              draw();
              setTimeout(() => ((lastKey = ""), draw()), CONFIRM_MS + 50);
            }
          });
        } else {
          action = h("button", { type: "button", class: "btn btn-small", disabled: busy != null }, isBusy ? "绑定中…" : "绑定到本机");
          action.addEventListener("click", () => run("绑定", w.address, () => api.bind(w.address)));
        }

        const sync = h("input", { type: "checkbox", checked: w.mb_light_sync, disabled: !ours || busy != null, "aria-label": `${w.kind} 主板灯光同步` });
        sync.addEventListener("change", () => run(sync.checked ? "开启主板灯光同步" : "关闭主板灯光同步", w.address, () => api.setMbLightSync(w.address, sync.checked)));

        const inLock = h("input", { type: "checkbox", checked: !w.excluded, disabled: !ours, "aria-label": `${w.kind} 受控` });
        inLock.addEventListener("change", () => {
          const set = new Set(locked);
          if (inLock.checked) set.add(w.address);
          else set.delete(w.address);
          setLocked([...set]);
        });

        return h(
          "tr",
          { class: w.excluded ? "row-muted" : "" },
          h("td", {}, w.fan_count ? `${w.kind} × ${w.fan_count}` : w.kind),
          h("td", { class: "mono" }, w.address),
          h("td", {}, w.owner === "other" ? `其他发射器 ${w.master}` : OWNER[w.owner]),
          h("td", { class: "mono" }, `${w.rx} / ${w.channel}`),
          h("td", {}, sync),
          locked.length > 0 ? h("td", {}, inLock) : null,
          h("td", {}, action),
        );
      }),
    );
    if (!st?.wireless.length) wlBody.replaceChildren(h("tr", {}, h("td", { colspan: 7, class: "muted" }, "没有收到无线设备（收发器没插，或者被 L-Connect 占用）")));

    usbBody.replaceChildren(
      ...(st?.devices ?? [])
        .filter((d) => d.connection.startsWith("USB"))
        .map((d) => h("tr", {}, h("td", {}, d.name), h("td", {}, d.connection), h("td", { class: "mono" }, d.address))),
    );
  }

  return { root, update: draw };
}
