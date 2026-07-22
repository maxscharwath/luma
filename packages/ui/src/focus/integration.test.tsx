// @vitest-environment jsdom
//
// The focus engine and <Focusable> exercised TOGETHER, which is what a screen
// actually does: a remote key press must move the ring, not merely move DOM
// focus. Asserted here rather than in a browser because focus events are not
// delivered reliably to a backgrounded tab, so a manual check can read as a bug
// that does not exist (and, worse, hide one that does).

import { cleanup, render, screen } from '@testing-library/react';
import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ring } from '../tokens';
import { Focusable } from './Focusable';
import { clearPressGuard } from './guard';
import { useFocusNav } from './nav.web';

/** jsdom gives every element a zero rect, and the navigator skips zero-sized
 * candidates, so the geometry is stubbed on the prototype BEFORE the first
 * render: mounting is itself a focus event (the screen focuses its entry point),
 * so placing rects afterwards would be too late. Two cells, side by side, keyed
 * off the label each focusable already carries. */
const LEFT_OF = { left: 0, right: 200 } as const;

function stubGeometry() {
  Element.prototype.getBoundingClientRect = function (this: Element): DOMRect {
    const label = this.getAttribute('aria-label') as keyof typeof LEFT_OF | null;
    const left = (label && LEFT_OF[label]) ?? 0;
    return {
      left,
      top: 0,
      width: 100,
      height: 100,
      right: left + 100,
      bottom: 100,
      x: left,
      y: 0,
      toJSON: () => ({}),
    } as DOMRect;
  };
}

function Screen() {
  useFocusNav({});
  return (
    <>
      <Focusable label="left" focusScale={1.06} />
      <Focusable label="right" />
    </>
  );
}

function press(k: string) {
  act(() => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));
  });
}

const style = (label: string) => screen.getByLabelText(label).style;

beforeEach(() => {
  stubGeometry();
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => {
  cleanup();
  clearPressGuard();
  vi.restoreAllMocks();
});

describe('a remote key press moves the ring, not just the focus', () => {
  it('rings the first focusable on mount and moves the ring on the D-pad', () => {
    const { container } = render(<Screen />);
    const [a, b] = Array.from(container.querySelectorAll('[data-focus]'));
    if (!a || !b) throw new Error('focusables not rendered');

    // Mount focuses the first focusable, and the ring follows the focus.
    expect(document.activeElement).toBe(a);
    expect(style('left').boxShadow.replace(/\s+/g, ' ')).toBe(ring.focusLift);
    expect(style('right').boxShadow).toBe('');

    press('ArrowRight');
    expect(document.activeElement).toBe(b);
    expect(style('right').boxShadow.replace(/\s+/g, ' ')).toBe(ring.focusLift);
    // The one we left must give the ring back, or every visited control keeps it.
    expect(style('left').boxShadow).toBe('');
  });

  it('keeps the ring and the scale together on a tile that scales', () => {
    const { container } = render(<Screen />);
    const [a, b] = Array.from(container.querySelectorAll('[data-focus]'));
    if (!a || !b) throw new Error('focusables not rendered');

    expect(style('left').transform).toContain('scale(1.06)');
    press('ArrowRight');
    expect(style('left').transform).toContain('scale(1)');
    expect(style('left').transform).not.toContain('scale(1.06)');
  });

  it('scrolls the newly focused control into view', () => {
    const { container } = render(<Screen />);
    const [a, b] = Array.from(container.querySelectorAll('[data-focus]'));
    if (!a || !b) throw new Error('focusables not rendered');

    press('ArrowRight');
    expect(b.scrollIntoView).toHaveBeenCalledWith({
      block: 'nearest',
      inline: 'nearest',
      behavior: 'smooth',
    });
  });
});
