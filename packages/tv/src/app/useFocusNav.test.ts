// @vitest-environment jsdom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useFocusNav } from '#tv/app/useFocusNav';

// jsdom returns a zero rect for every element and doesn't implement
// scrollIntoView, so we stub both: each focusable gets a hand-placed rect and
// scrollIntoView is a no-op (spatial nav calls it after every move).
function rect(left: number, top: number, w: number, h: number): DOMRect {
  return {
    left,
    top,
    width: w,
    height: h,
    right: left + w,
    bottom: top + h,
    x: left,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

function focusable(id: string, r: DOMRect): HTMLButtonElement {
  const el = document.createElement('button');
  el.id = id;
  el.setAttribute('data-focus', '');
  el.getBoundingClientRect = () => r;
  document.body.appendChild(el);
  return el;
}

/** A 2x2 grid of focusables:  a b / c d  (100px cells, 100px gaps). */
function grid2x2() {
  const a = focusable('a', rect(0, 0, 100, 100));
  const b = focusable('b', rect(200, 0, 100, 100));
  const c = focusable('c', rect(0, 200, 100, 100));
  const d = focusable('d', rect(200, 200, 100, 100));
  return { a, b, c, d };
}

function key(k: string, init: KeyboardEventInit = {}) {
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...init }),
    );
  });
}

beforeEach(() => {
  document.body.innerHTML = '';
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => {
  cleanup();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe('useFocusNav mount focus', () => {
  it('focuses the first focusable when nothing is focused yet', () => {
    grid2x2();
    renderHook(() => useFocusNav({}));
    expect(document.activeElement?.id).toBe('a');
  });

  it('does not steal focus if a focusable is already active', () => {
    const { b } = grid2x2();
    b.focus();
    renderHook(() => useFocusNav({}));
    expect(document.activeElement?.id).toBe('b');
  });
});

describe('useFocusNav spatial movement', () => {
  it('moves to the nearest neighbour in each direction', () => {
    grid2x2();
    renderHook(() => useFocusNav({}));
    expect(document.activeElement?.id).toBe('a');
    key('ArrowRight');
    expect(document.activeElement?.id).toBe('b');
    key('ArrowDown');
    expect(document.activeElement?.id).toBe('d');
    key('ArrowLeft');
    expect(document.activeElement?.id).toBe('c');
    key('ArrowUp');
    expect(document.activeElement?.id).toBe('a');
  });

  it('leaves focus put when there is no candidate in that direction', () => {
    grid2x2();
    renderHook(() => useFocusNav({}));
    key('ArrowLeft'); // nothing to the left of "a"
    expect(document.activeElement?.id).toBe('a');
    key('ArrowUp'); // nothing above "a"
    expect(document.activeElement?.id).toBe('a');
  });
});

describe('useFocusNav handlers', () => {
  it('invokes onBack on Back and onPlayPause on a media key', () => {
    grid2x2();
    const onBack = vi.fn();
    const onPlayPause = vi.fn();
    renderHook(() => useFocusNav({ onBack, onPlayPause }));
    key('Escape'); // -> Back
    expect(onBack).toHaveBeenCalledTimes(1);
    key('MediaPlayPause'); // -> PlayPause
    expect(onPlayPause).toHaveBeenCalledTimes(1);
  });

  it('OK clicks the focused element once the mount guard elapses', () => {
    vi.useFakeTimers();
    const { a } = grid2x2();
    const onClick = vi.fn();
    a.addEventListener('click', onClick);
    renderHook(() => useFocusNav({}));
    expect(document.activeElement?.id).toBe('a');
    // Within the 300ms OK-guard the press is swallowed (the tail of the press
    // that opened this screen).
    key('Enter');
    expect(onClick).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(301));
    key('Enter');
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});

describe('useFocusNav text-field handling', () => {
  it('lets a focused input own ◀ ▶ but ▲ ▼ still move focus out', () => {
    const input = document.createElement('input');
    input.id = 'field';
    input.setAttribute('data-focus', '');
    input.getBoundingClientRect = () => rect(0, 200, 100, 100);
    document.body.appendChild(input);
    focusable('above', rect(0, 0, 100, 100));
    input.focus();
    renderHook(() => useFocusNav({}));
    expect(document.activeElement?.id).toBe('field');
    key('ArrowLeft'); // native cursor move, focus must stay in the field
    expect(document.activeElement?.id).toBe('field');
    key('ArrowUp'); // leaves the field to the focusable above
    expect(document.activeElement?.id).toBe('above');
  });
});

describe('useFocusNav pointer environment', () => {
  // Hover-focus was removed on request: the ring moves on D-pad/arrows only,
  // a mouse interacts by clicking. The hook no longer reads the input
  // environment at all, so hover never moves focus, pointer or not.
  it('hover does not change focus', () => {
    const { a, b } = grid2x2();
    a.focus();
    renderHook(() => useFocusNav({}));
    act(() => b.dispatchEvent(new Event('pointerover', { bubbles: true })));
    expect(document.activeElement?.id).toBe('a');
  });
});
