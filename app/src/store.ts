import { api, type Config, type Status } from "./api";

/** 走势图保留的样本数（每秒一个）。 */
const HISTORY_LEN = 120;

export interface History { coolant: number[]; pump: number[]; fan: number[]; cpu: number[]; gpu: number[] }

export const store = {
  status: null as Status | null,
  config: null as Config | null,
  /** 连不上后台服务时的错误信息。 */
  offline: null as string | null,
  history: { coolant: [], pump: [], fan: [], cpu: [], gpu: [] } as History,
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
  } catch (e) {
    store.offline = String(e);
    store.config = null;
  }
  emit();
}

export function startPolling() {
  poll();
  setInterval(poll, 1000);
}

/** 写入后台服务，并以服务规范化后的版本为准。 */
export async function saveConfig(config: Config) {
  await api.setConfig(config);
  store.config = await api.getConfig();
  emit();
}
