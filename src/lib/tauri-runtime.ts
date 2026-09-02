export function isTauriRuntime(): boolean {
  const runtimeWindow = window as Window & { __TAURI_INTERNALS__?: unknown; __TAURI__?: unknown };
  return runtimeWindow.__TAURI_INTERNALS__ != null || runtimeWindow.__TAURI__ != null;
}
