// @vitest-environment jsdom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EXIT_MS } from './constants';
import { useIntroExit } from './useIntroExit';
import { useIntroKeys } from './useIntroKeys';

// The hooks own document/window listeners, so a hook left mounted by one test
// would keep eating the next one's keys (the skip handler stops propagation).
afterEach(cleanup);

/** Press a key the way a keyboard / TV remote does: from the focused element,
 * so the window capture phase runs before the document listeners. */
function press(key: string) {
  const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
  act(() => {
    document.body.dispatchEvent(e);
  });
  return e;
}

describe('useIntroExit', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('fades, then hands off exactly once after EXIT_MS', () => {
    const onDone = vi.fn();
    const { result } = renderHook(() => useIntroExit(onDone));

    expect(result.current.exiting).toBe(false);
    act(() => result.current.exit());
    expect(result.current.exiting).toBe(true);
    expect(onDone).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(EXIT_MS));
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('is single-shot: a second exit (a late `ended` after a skip) is ignored', () => {
    const onDone = vi.fn();
    const { result } = renderHook(() => useIntroExit(onDone));

    act(() => result.current.exit());
    act(() => result.current.exit());
    act(() => vi.advanceTimersByTime(EXIT_MS * 2));
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('reopen() cancels a pending hand-off and re-arms the run (replay)', () => {
    const onDone = vi.fn();
    const { result } = renderHook(() => useIntroExit(onDone));

    act(() => result.current.exit());
    act(() => result.current.reopen());
    expect(result.current.exiting).toBe(false);
    expect(result.current.exitedRef.current).toBe(false);

    act(() => vi.advanceTimersByTime(EXIT_MS * 2));
    expect(onDone).not.toHaveBeenCalled();

    // and the run can end again afterwards
    act(() => result.current.exit());
    act(() => vi.advanceTimersByTime(EXIT_MS));
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('clearTimers() drops the pending hand-off (unmount cleanup)', () => {
    const onDone = vi.fn();
    const { result } = renderHook(() => useIntroExit(onDone));

    act(() => result.current.exit());
    act(() => result.current.clearTimers());
    act(() => vi.advanceTimersByTime(EXIT_MS * 2));
    expect(onDone).not.toHaveBeenCalled();
  });
});

describe('useIntroKeys', () => {
  const keys = () => {
    const exit = vi.fn();
    const replay = vi.fn();
    const unblock = vi.fn();
    const view = renderHook(() => useIntroKeys({ exit, replay, unblock }));
    return { exit, replay, unblock, ...view };
  };

  it.each(['Enter', ' ', 'Spacebar', 'Escape', 'GoBack', 'BrowserBack'])(
    'skips on %j without also unblocking',
    (key) => {
      const { exit, replay, unblock } = keys();
      const e = press(key);
      expect(exit).toHaveBeenCalledTimes(1);
      expect(replay).not.toHaveBeenCalled();
      // stopImmediatePropagation keeps the skip key away from the unblock
      // listener: skipping must never restart the intro.
      expect(unblock).not.toHaveBeenCalled();
      expect(e.defaultPrevented).toBe(true);
    },
  );

  it('replays on r / R', () => {
    const { exit, replay, unblock } = keys();
    press('r');
    press('R');
    expect(replay).toHaveBeenCalledTimes(2);
    expect(exit).not.toHaveBeenCalled();
    expect(unblock).not.toHaveBeenCalled();
  });

  it('routes any other key (arrows, letters) to unblock only', () => {
    const { exit, replay, unblock } = keys();
    press('ArrowRight');
    press('a');
    expect(unblock).toHaveBeenCalledTimes(2);
    expect(exit).not.toHaveBeenCalled();
    expect(replay).not.toHaveBeenCalled();
  });

  it('unblocks on a pointer gesture', () => {
    const { unblock } = keys();
    act(() => {
      document.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    });
    expect(unblock).toHaveBeenCalledTimes(1);
  });

  it('calls the latest closures without re-registering', () => {
    const exit = vi.fn();
    const later = vi.fn();
    const noop = () => undefined;
    const { rerender } = renderHook(
      (p: { exit: () => void }) => useIntroKeys({ ...p, replay: noop, unblock: noop }),
      { initialProps: { exit } },
    );
    rerender({ exit: later });
    press('Enter');
    expect(exit).not.toHaveBeenCalled();
    expect(later).toHaveBeenCalledTimes(1);
  });

  it('stops listening once unmounted', () => {
    const { exit, unblock, unmount } = keys();
    unmount();
    press('Enter');
    press('ArrowUp');
    expect(exit).not.toHaveBeenCalled();
    expect(unblock).not.toHaveBeenCalled();
  });
});
