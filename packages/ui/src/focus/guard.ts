// The OK guard. Platform-neutral: both focus engines arm it, every <Focusable>
// consults it.
//
// Don't let one physical OK carry past the screen it opened. The press that
// navigates somewhere (card -> detail) would otherwise also fire the control the
// new screen auto-focuses (detail -> Play -> player), via the remote's key repeat
// or a keyup/keydown bounce. So presses are ignored for a short window after
// every screen mounts: long enough to swallow the stray repeat, short enough
// that a deliberate second press still lands.
//
// Module scope so it survives the transition's unmount/mount.

let guardedUntil = 0;

/** Default window, in milliseconds. */
export const PRESS_GUARD_MS = 300;

/** Arm the guard. Called by every screen's `useFocusNav` on mount. */
export function armPressGuard(ms: number = PRESS_GUARD_MS): void {
  guardedUntil = Date.now() + ms;
}

/** True while a press should be swallowed as the tail of the previous screen's. */
export function pressGuardActive(): boolean {
  return Date.now() < guardedUntil;
}

/** Test seam: drop the guard so a press lands immediately. */
export function clearPressGuard(): void {
  guardedUntil = 0;
}
