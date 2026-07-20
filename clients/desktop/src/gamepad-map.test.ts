import { describe, expect, it } from 'vitest';
import { activeKeys, freshPadState, type PadState, updateCalibration } from './gamepad-map';

// Pure-logic tests for the gamepad -> key translation: the standard W3C layout,
// the raw evdev layout WebKitGTK can expose (D-pad as hat axes, triggers as axes
// resting at -1), and the rest-calibration that keeps both from emitting garbage.

type ButtonSpec = number | { pressed: boolean; value: number };

function makePad(opts: { mapping?: string; buttons?: ButtonSpec[]; axes?: number[] }): Gamepad {
  const buttons = (opts.buttons ?? []).map((b) =>
    typeof b === 'number'
      ? { pressed: b >= 0.5, touched: b > 0, value: b }
      : { pressed: b.pressed, touched: b.pressed || b.value > 0, value: b.value },
  );
  return {
    id: 'test-pad',
    index: 0,
    connected: true,
    timestamp: 0,
    mapping: opts.mapping ?? 'standard',
    buttons,
    axes: opts.axes ?? [0, 0, 0, 0],
  } as unknown as Gamepad;
}

/** One bridge frame: calibrate then read, like the tick loop does. */
function frame(pad: Gamepad, state: PadState): string[] {
  updateCalibration(pad, state);
  return [...activeKeys(pad, state)].sort();
}

const zeros = (n: number): number[] => new Array(n).fill(0);

describe('standard mapping', () => {
  it('maps D-pad buttons 12-15 to directions', () => {
    const state = freshPadState('test-pad');
    frame(makePad({ buttons: zeros(16) }), state);
    const buttons = zeros(16);
    buttons[13] = 1;
    expect(frame(makePad({ buttons }), state)).toEqual(['Down']);
  });

  it('maps A/B/X and bumpers', () => {
    const state = freshPadState('test-pad');
    frame(makePad({ buttons: zeros(16) }), state);
    const buttons = zeros(16);
    buttons[0] = 1;
    buttons[5] = 1;
    expect(frame(makePad({ buttons }), state)).toEqual(['Enter', 'FastForward']);
  });

  it('ignores a feather-touched trigger but honors a real pull', () => {
    const state = freshPadState('test-pad');
    frame(makePad({ buttons: zeros(16) }), state);
    // Some backends flip `pressed` at a feather touch: must NOT seek.
    const feather = zeros(16) as ButtonSpec[];
    feather[7] = { pressed: true, value: 0.2 };
    expect(frame(makePad({ buttons: feather }), state)).toEqual([]);
    const pulled = zeros(16) as ButtonSpec[];
    pulled[7] = { pressed: true, value: 0.8 };
    expect(frame(makePad({ buttons: pulled }), state)).toEqual(['FastForward']);
  });

  it('turns a diagonal left-stick push into the single dominant direction', () => {
    const state = freshPadState('test-pad');
    const pad = makePad({ buttons: zeros(16), axes: [0.9, 0.8, 0, 0] });
    frame(makePad({ buttons: zeros(16) }), state); // calibrate at rest first
    expect(frame(pad, state)).toEqual(['Right']);
  });
});

describe('raw (non-standard) layout', () => {
  // evdev order: axes 0/1 left stick, 2 LT, 3/4 right stick, 5 RT, 6/7 hat D-pad;
  // triggers REST at -1.
  const restAxes = [0, 0, -1, 0, 0, -1, 0, 0];

  it('emits nothing at rest despite trigger axes stuck at -1', () => {
    const state = freshPadState('test-pad');
    expect(frame(makePad({ mapping: '', buttons: zeros(11), axes: restAxes }), state)).toEqual([]);
  });

  it('reads the D-pad from the hat axis pair 6/7', () => {
    const state = freshPadState('test-pad');
    frame(makePad({ mapping: '', buttons: zeros(11), axes: restAxes }), state);
    const axes = [...restAxes];
    axes[6] = 1; // hat right
    expect(frame(makePad({ mapping: '', buttons: zeros(11), axes }), state)).toEqual(['Right']);
    axes[6] = 0;
    axes[7] = -1; // hat up
    expect(frame(makePad({ mapping: '', buttons: zeros(11), axes }), state)).toEqual(['Up']);
  });

  it('does not fire seek keys on Select/Start (raw indices 6/7)', () => {
    const state = freshPadState('test-pad');
    frame(makePad({ mapping: '', buttons: zeros(11), axes: restAxes }), state);
    const buttons = zeros(11);
    buttons[6] = 1;
    buttons[7] = 1;
    expect(frame(makePad({ mapping: '', buttons, axes: restAxes }), state)).toEqual([]);
  });

  it('still maps face buttons and bumpers', () => {
    const state = freshPadState('test-pad');
    frame(makePad({ mapping: '', buttons: zeros(11), axes: restAxes }), state);
    const buttons = zeros(11);
    buttons[1] = 1; // B -> back
    buttons[4] = 1; // L1 -> rewind
    expect(frame(makePad({ mapping: '', buttons, axes: restAxes }), state)).toEqual([
      'Back',
      'Rewind',
    ]);
  });
});

describe('rest calibration', () => {
  it('never trusts an axis that has not been seen at rest', () => {
    const state = freshPadState('test-pad');
    const stuck = [0, 0, 0, 0, 0, 0, -1, 0]; // hat X stuck at -1 since connect
    expect(frame(makePad({ mapping: '', buttons: zeros(11), axes: stuck }), state)).toEqual([]);
    // Once it has been observed neutral, it becomes a live D-pad axis.
    frame(makePad({ mapping: '', buttons: zeros(11), axes: zeros(8) }), state);
    expect(frame(makePad({ mapping: '', buttons: zeros(11), axes: stuck }), state)).toEqual([
      'Left',
    ]);
  });

  it('never trusts a button that has not been seen released', () => {
    const state = freshPadState('test-pad');
    const buttons = zeros(16);
    buttons[0] = 1; // held since before the pad was detected
    expect(frame(makePad({ buttons }), state)).toEqual([]);
    frame(makePad({ buttons: zeros(16) }), state);
    expect(frame(makePad({ buttons }), state)).toEqual(['Enter']);
  });
});

describe('single-axis hat (axis 9)', () => {
  const rest = () => {
    const axes = zeros(10);
    axes[9] = 1.2857; // classic out-of-range rest value
    return axes;
  };

  it('decodes the 8-way notches once the rest signature has been seen', () => {
    const state = freshPadState('test-pad');
    frame(makePad({ mapping: '', buttons: zeros(11), axes: rest() }), state);
    for (const [v, keys] of [
      [-1, ['Up']],
      [-3 / 7, ['Right']],
      [1 / 7, ['Down']],
      [5 / 7, ['Left']],
      [-5 / 7, ['Right', 'Up']],
    ] as const) {
      const axes = rest();
      axes[9] = v;
      expect(frame(makePad({ mapping: '', buttons: zeros(11), axes }), state)).toEqual([...keys]);
    }
  });

  it('ignores axis 9 without the rest signature (could be a real stick)', () => {
    const state = freshPadState('test-pad');
    const axes = zeros(10);
    axes[9] = 1; // looks like the up-left notch, but never seen at rest
    expect(frame(makePad({ mapping: '', buttons: zeros(11), axes }), state)).toEqual([]);
  });

  it('ignores off-notch values like a resting 0', () => {
    const state = freshPadState('test-pad');
    frame(makePad({ mapping: '', buttons: zeros(11), axes: rest() }), state);
    const axes = zeros(10); // axis 9 at 0 is between notches, must not read as Down
    expect(frame(makePad({ mapping: '', buttons: zeros(11), axes }), state)).toEqual([]);
  });
});
