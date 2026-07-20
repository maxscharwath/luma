import { getTauri } from '#tv/features/playback/player/engine';

/**
 * Webview GPU-rendering toggle, Linux desktop shell only: it drives the
 * WebKitGTK DMABUF renderer opt-in persisted by the Rust side (webview_gpu.rs).
 * No other shell has that knob.
 */
export function gpuToggleAvailable(): boolean {
  if (getTauri() == null) return false;
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  return /Linux/i.test(ua) && !/Android/i.test(ua);
}

/** The persisted choice (false when unset or when the shell can't answer). */
export async function getGpuRendering(): Promise<boolean> {
  try {
    return (await getTauri()?.core.invoke('webview_gpu_get')) === true;
  } catch {
    return false;
  }
}

/**
 * Persist the choice, then relaunch: the renderer is picked before the webview
 * initialises, so a flip can only take effect on a fresh boot.
 */
export async function setGpuRendering(enabled: boolean): Promise<void> {
  const tauri = getTauri();
  if (!tauri) return;
  try {
    await tauri.core.invoke('webview_gpu_set', { enabled });
    await tauri.core.invoke('app_relaunch');
  } catch {
    /* the row stays usable; the next toggle retries */
  }
}
