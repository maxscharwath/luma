import { getExo, getTauri } from '#tv/features/playback/player/engine';

/** Whether the hosting shell can terminate the whole app. Two shells qualify:
 * the desktop (Tauri) shell and the Android TV shell - both run fullscreen
 * without window chrome, so the UI must offer the way out itself. Real TVs
 * (Tizen/webOS) quit through their own system UI, so the row stays hidden. */
export function canQuitApp(): boolean {
  return getTauri() != null || typeof getExo()?.quit === 'function';
}

/** Ask the hosting shell to close the app: the desktop `app_quit` command (which
 * exits through the event loop and so also stops the mpv sidecar), or the
 * Android bridge's `quit` (finishes the activity and clears the task). */
export function quitApp(): void {
  const tauri = getTauri();
  if (tauri != null) {
    void tauri.core.invoke('app_quit');
    return;
  }
  getExo()?.quit?.();
}
