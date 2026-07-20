import { getTauri } from '#tv/features/playback/player/engine';

/** Whether the hosting shell can terminate the whole app. Only the desktop
 * (Tauri) shell qualifies: it runs fullscreen without window chrome, so the UI
 * must offer the way out itself. Real TVs quit through their own system UI. */
export function canQuitApp(): boolean {
  return getTauri() != null;
}

/** Ask the hosting shell to close the app (the desktop `app_quit` command,
 * which exits through the event loop and so also stops the mpv sidecar). */
export function quitApp(): void {
  void getTauri()?.core.invoke('app_quit');
}
