import type { RemoteKey } from '@luma/core';

// Gamepad -> TV navigation bridge (@luma/desktop; Steam Deck the primary target).
//
// The shared @luma/tv nav (useFocusNav) and player (usePlayerControls,
// useDirectPlayback) all listen for `keydown` / `keyup` on `window` and normalize
// them with `resolveRemoteKey` (packages/core/src/remote.ts), which resolves by
// `KeyboardEvent.key` first. So the entire 10-foot input model is already
// keyboard-shaped: we just poll the Gamepad API and dispatch the matching synthetic
// key events on `window`. Nothing in @luma/tv has to change.

// Only the logical keys we emit. Each value is the `KeyboardEvent.key` string that
// `resolveRemoteKey`'s KEY_NAMES table recognizes; that lookup wins, so we don't
// need the legacy `keyCode` (which browsers ignore in the KeyboardEvent ctor anyway).
type EmitKey = Extract<
  RemoteKey,
  'Up' | 'Down' | 'Left' | 'Right' | 'Enter' | 'Back' | 'PlayPause' | 'Rewind' | 'FastForward'
>;

const KEY_NAME: Record<EmitKey, string> = {
  Up: 'ArrowUp',
  Down: 'ArrowDown',
  Left: 'ArrowLeft',
  Right: 'ArrowRight',
  Enter: 'Enter',
  Back: 'Escape',
  PlayPause: 'MediaPlayPause',
  Rewind: 'MediaRewind',
  FastForward: 'MediaFastForward',
};

// Directions and seek auto-repeat while held (continuous scroll / accelerating
// seek, like a held remote arrow). Discrete actions must NOT repeat: nav already
// swallows a held Enter, but a repeated Back would pop several screens at once.
const REPEATABLE: ReadonlySet<EmitKey> = new Set([
  'Up',
  'Down',
  'Left',
  'Right',
  'Rewind',
  'FastForward',
]);

// Standard W3C gamepad button layout. Steam Input presents the Deck this way (both
// its built-in pad and a paired controller), so we target the standard mapping.
const BUTTON_TO_KEY: Readonly<Record<number, EmitKey>> = {
  0: 'Enter', // A  (South) - select / OK
  1: 'Back', // B  (East)  - back
  2: 'PlayPause', // X  (West)  - play/pause
  4: 'Rewind', // L1 - seek back
  5: 'FastForward', // R1 - seek forward
  6: 'Rewind', // L2
  7: 'FastForward', // R2
  8: 'Back', // View / Select
  12: 'Up', // D-pad up
  13: 'Down', // D-pad down
  14: 'Left', // D-pad left
  15: 'Right', // D-pad right
};

const STICK_DEADZONE = 0.5; // left-stick push past this counts as a direction
const REPEAT_DELAY_MS = 400; // hold this long before the first auto-repeat
const REPEAT_EVERY_MS = 120; // then repeat this often

function now(): number {
  return typeof performance !== 'undefined' ? performance.now() : Date.now();
}

function fire(type: 'keydown' | 'keyup', k: EmitKey, repeat: boolean): void {
  window.dispatchEvent(
    new KeyboardEvent(type, { key: KEY_NAME[k], bubbles: true, cancelable: true, repeat }),
  );
}

/** The keys a pad is asserting this frame (buttons + left stick as a D-pad). */
function activeKeys(pad: Gamepad): Set<EmitKey> {
  const keys = new Set<EmitKey>();
  pad.buttons.forEach((b, i) => {
    const k = BUTTON_TO_KEY[i];
    if (k && b.pressed) keys.add(k);
  });
  // Left stick -> one dominant direction (a diagonal push must not fire two keys).
  const x = pad.axes[0] ?? 0;
  const y = pad.axes[1] ?? 0;
  if (Math.abs(x) > STICK_DEADZONE || Math.abs(y) > STICK_DEADZONE) {
    if (Math.abs(x) > Math.abs(y)) keys.add(x < 0 ? 'Left' : 'Right');
    else keys.add(y < 0 ? 'Up' : 'Down');
  }
  return keys;
}

/**
 * Start translating connected gamepads into TV key events. Safe to call once at
 * boot; a no-op (returns an empty stopper) where the Gamepad API is absent.
 * Returns a stop function.
 */
export function startGamepadBridge(): () => void {
  if (typeof navigator === 'undefined' || typeof navigator.getGamepads !== 'function') {
    return () => {};
  }
  // Per-key hold state: when its next auto-repeat is due.
  const held = new Map<EmitKey, { nextRepeat: number }>();
  let raf = 0;
  let stopped = false;

  const tick = () => {
    if (stopped) return;
    const active = new Set<EmitKey>();
    for (const pad of navigator.getGamepads()) {
      if (!pad) continue;
      for (const k of activeKeys(pad)) active.add(k);
    }
    const t = now();

    // Newly pressed -> keydown; still-held repeatable key past its due time -> repeat.
    for (const k of active) {
      const state = held.get(k);
      if (!state) {
        fire('keydown', k, false);
        held.set(k, { nextRepeat: t + REPEAT_DELAY_MS });
      } else if (REPEATABLE.has(k) && t >= state.nextRepeat) {
        fire('keydown', k, true);
        state.nextRepeat = t + REPEAT_EVERY_MS;
      }
    }
    // Released -> keyup (drives e.g. the player's commit-seek-on-release).
    for (const k of held.keys()) {
      if (!active.has(k)) {
        fire('keyup', k, false);
        held.delete(k);
      }
    }
    raf = requestAnimationFrame(tick);
  };
  raf = requestAnimationFrame(tick);

  return () => {
    stopped = true;
    cancelAnimationFrame(raf);
  };
}
