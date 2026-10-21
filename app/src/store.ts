import { api, type Config, type Status } from "./api";
import { toast } from "./ui";

/** 走势图保留的样本数（每秒一个）。 */
const HISTORY_LEN = 120;

export interface History { coolant: number[]; pump: number[]; fan: number[]; cpu: number[]; gpu: number[]; cpu_temp: number[] }

export const store = {
  status: null as Status | null,
  config: null as Config | null,
  /** 连不上后台服务时的错误信息。 */
  offline: null as string | null,
  history: { coolant: [], pump: [], fan: [], cpu: [], gpu: [], cpu_temp: [] } as History,
};

const listeners = new Set<() => void>();

export function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function emit() {
  listeners.forEach((fn) => fn());
}

function push(arr: number[], v: number | null) {
  if (v == null) return;
  arr.push(v);
  if (arr.length > HISTORY_LEN) arr.shift();
}

async function poll() {
  try {
    const status = await api.getStatus();
    if (!store.config) store.config = await api.getConfig();
    store.status = status;
    store.offline = null;
    push(store.history.coolant, status.coolant_c);
    push(store.history.pump, status.pump_rpm);
    push(store.history.fan, status.fan_percent);
    push(store.history.cpu, status.cpu_usage);
    push(store.history.gpu, status.gpu_temp_c);
    push(store.history.cpu_temp, status.cpu_temp_c ?? null);
  } catch (e) {
    store.offline = String(e);
    store.config = null;
  }
  emit();
}

export async function startPolling() {
  // 先拿后台服务记下的最近 2 分钟，走势图一打开就是满的；拿不到（服务没运行）就从空开始记。
  try {
    const h = await api.getHistory();
    for (const k of Object.keys(store.history) as (keyof History)[]) store.history[k] = (h[k] ?? []).slice(-HISTORY_LEN);
  } catch {
    /* 忽略 */
  }
  poll();
  setInterval(poll, 1000);
}

/** 写入后台服务，并以服务规范化后的版本为准。 */
export async function saveConfig(config: Config) {
  await api.setConfig(config);
  store.config = await api.getConfig();
  emit();
}

/**
 * 改动立即生效：先在界面上更新，再写入后台服务，然后弹出“已应用 · 撤销”。
 * 撤销会写回改动前的整份配置。`key` 相同的提示互相替换（连续拖同一个滑块只留一条）。
 */
export async function commit(next: Config, label: string, key = label) {
  const prev = store.config ? structuredClone(store.config) : null;
  store.config = structuredClone(next);
  emit();
  try {
    await saveConfig(next);
    toast(`已应用：${label}`, {
      key,
      undo: prev
        ? () => {
            saveConfig(prev)
              .then(() => toast(`已撤销：${label}`, { key }))
              .catch((e) => toast(`撤销失败：${e}`, { error: true, key }));
          }
        : undefined,
    });
  } catch (e) {
    if (prev) store.config = prev;
    emit();
    toast(`应用失败：${e}`, { error: true, key });
  }
}

/** 在当前配置上改一处并提交。 */
export function edit(label: string, mutate: (c: Config) => void, key?: string) {
  if (!store.config) return;
  const next = structuredClone(store.config);
  mutate(next);
  return commit(next, label, key);
}
