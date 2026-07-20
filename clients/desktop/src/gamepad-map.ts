import type { RemoteKey } from '@kroma/core';

// Pure gamepad decoding for the desktop bridge (see gamepad.ts for the loop).
//
// Real pads are messier than the W3C `standard` layout, especially under WebKitGTK
// (the Deck's Tauri webview), which can expose the raw evdev layout instead:
//  - the D-pad becomes a hat AXIS pair (typically axes 6/7), not buttons 12-15
//  - analog triggers become axes resting at -1, or buttons whose `pressed` flips
//    on a feather touch
//  - button indices past the face buttons shift (6/7 may be Select/Start, not L2/R2)
// So: buttons/axes are only honored once seen at rest (kills anything stuck at -1),
// triggers need a real >= 0.5 pull, non-standard pads get a conservative button
// table plus hat-axis D-pad decoding.

// Only the logical keys we emit. Each value is the `KeyboardEvent.key` string that
// `resolveRemoteKey`'s KEY_NAMES table recognizes; that lookup wins, so we don't
// need the legacy `keyCode` (which browsers ignore in the KeyboardEvent ctor anyway).
export type EmitKey = Extract<
  RemoteKey,
  'Up' | 'Down' | 'Left' | 'Right' | 'Enter' | 'Back' | 'PlayPause' | 'Rewind' | 'FastForward'
>;

export const KEY_NAME: Record<EmitKey, string> = {
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
export const REPEATABLE: ReadonlySet<EmitKey> = new Set([
  'Up',
  'Down',
  'Left',
  'Right',
  'Rewind',
  'FastForward',
]);

// Standard W3C layout, used when the pad declares `mapping === 'standard'`.
const STANDARD_BUTTONS: Readonly<Record<number, EmitKey>> = {
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

// Unknown layout: only the indices that hold across the common evdev/SDL orders
// (face buttons + bumpers). 6/7/8 are deliberately NOT mapped - depending on the
// backend they are Select/Start/Guide or analog triggers, and firing seek keys on
// those is exactly the "trigger does random things" failure. 12-15 kept as a
// harmless bonus for layouts that do put the D-pad there.
const FALLBACK_BUTTONS: Readonly<Record<number, EmitKey>> = {
  0: 'Enter',
  1: 'Back',
  2: 'PlayPause',
  4: 'Rewind',
  5: 'FastForward',
  12: 'Up',
  13: 'Down',
  14: 'Left',
  15: 'Right',
};

// Analog triggers in the standard layout: honored only on a real pull, never on
// `pressed` alone (some backends flip `pressed` at a feather touch).
const STANDARD_TRIGGERS: ReadonlySet<number> = new Set([6, 7]);

const STICK_DEADZONE = 0.5; // stick/hat push past this counts as a direction
const TRIGGER_PULL = 0.5; // analog trigger value that counts as pressed
const NEUTRAL_AXIS = 0.15; // |axis| below this counts as "seen at rest"
const NEUTRAL_BUTTON = 0.1; // button value below this counts as "seen at rest"

// Per-pad calibration: an input is only trusted after it has been observed at
// rest once. A trigger axis resting at -1 (raw evdev) or a miscalibrated button
// never qualifies, so it can never emit phantom keys.
export interface PadState {
  id: string;
  axisReady: boolean[];
  buttonReady: boolean[];
  hat9Ready: boolean; // axis 9 seen at its out-of-range rest value (classic hat)
}

export function freshPadState(id: string): PadState {
  return { id, axisReady: [], buttonReady: [], hat9Ready: false };
}

function dominantDirection(x: number, y: number, keys: Set<EmitKey>): void {
  if (Math.abs(x) <= STICK_DEADZONE && Math.abs(y) <= STICK_DEADZONE) return;
  // One dominant direction - a diagonal push must not fire two keys.
  if (Math.abs(x) > Math.abs(y)) keys.add(x < 0 ? 'Left' : 'Right');
  else keys.add(y < 0 ? 'Up' : 'Down');
}

// Classic single-axis hat encoding (axis 9 on 10-axis raw layouts): 8 positions
// from -1 (up) clockwise in steps of 2/7, resting OUT of range (~1.29). Only
// decoded once that rest value has been seen, so a stick maxing at exactly 1.0
// can never be mistaken for it.
const HAT9_DIRS: ReadonlyArray<ReadonlyArray<EmitKey>> = [
  ['Up'],
  ['Up', 'Right'],
  ['Right'],
  ['Down', 'Right'],
  ['Down'],
  ['Down', 'Left'],
  ['Left'],
  ['Up', 'Left'],
];

function decodeHat9(v: number, keys: Set<EmitKey>): void {
  if (Math.abs(v) > 1.001) return; // at rest
  const step = Math.round(((v + 1) * 7) / 2);
  const exact = (step * 2) / 7 - 1;
  if (Math.abs(v - exact) > 0.05) return; // not on an 8-way notch
  for (const k of HAT9_DIRS[step] ?? []) keys.add(k);
}

export function updateCalibration(pad: Gamepad, state: PadState): void {
  pad.axes.forEach((v, i) => {
    if (Math.abs(v) < NEUTRAL_AXIS) state.axisReady[i] = true;
  });
  pad.buttons.forEach((b, i) => {
    if (!b.pressed && b.value < NEUTRAL_BUTTON) state.buttonReady[i] = true;
  });
  if (Math.abs(pad.axes[9] ?? 0) > 1.001) state.hat9Ready = true;
}

/** The keys a pad is asserting this frame (buttons + left stick + hat D-pad). */
export function activeKeys(pad: Gamepad, state: PadState): Set<EmitKey> {
  const keys = new Set<EmitKey>();
  const standard = pad.mapping === 'standard';
  const table = standard ? STANDARD_BUTTONS : FALLBACK_BUTTONS;

  pad.buttons.forEach((b, i) => {
    const k = table[i];
    if (!k || !state.buttonReady[i]) return;
    const pressed =
      standard && STANDARD_TRIGGERS.has(i)
        ? b.value >= TRIGGER_PULL
        : b.pressed || b.value >= TRIGGER_PULL;
    if (pressed) keys.add(k);
  });

  // Left stick as a D-pad.
  const x = state.axisReady[0] ? (pad.axes[0] ?? 0) : 0;
  const y = state.axisReady[1] ? (pad.axes[1] ?? 0) : 0;
  dominantDirection(x, y, keys);

  // Raw layouts expose the D-pad as a hat axis pair (6/7 after sticks+triggers).
  // A true standard pad has exactly 4 axes, so length gates this, not `mapping`.
  if (pad.axes.length >= 8) {
    const hx = state.axisReady[6] ? (pad.axes[6] ?? 0) : 0;
    const hy = state.axisReady[7] ? (pad.axes[7] ?? 0) : 0;
    dominantDirection(hx, hy, keys);
  }
  if (pad.axes.length >= 10 && state.hat9Ready) decodeHat9(pad.axes[9] ?? 0, keys);

  return keys;
}
