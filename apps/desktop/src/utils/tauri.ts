import {
  convertFileSrc as tauriConvertFileSrc,
  invoke as tauriInvoke,
  isTauri as tauriIsTauri,
} from "@tauri-apps/api/core";

type InvokeArgs = Record<string, unknown>;

/** 检测当前是否在 Tauri WebView 内运行 */
export function isTauri(): boolean {
  return tauriIsTauri() || typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

/**
 * 安全调用 Tauri 后端命令
 * 在非 Tauri 环境抛出友好错误，而不是 undefined 异常
 */
export async function invoke<T>(cmd: string, args?: InvokeArgs): Promise<T> {
  if (!isTauri()) {
    const msg = `[QuickNote] 当前不在 Tauri 环境中运行，命令 "${cmd}" 不可用。请通过 "npm run tauri dev" 启动应用。`;
    console.warn(msg);
    throw new Error(msg);
  }

  return tauriInvoke<T>(cmd, args ?? {});
}

export function convertFileSrc(filePath: string): string {
  if (!isTauri()) return filePath;
  return tauriConvertFileSrc(filePath);
}
