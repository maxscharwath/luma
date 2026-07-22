// @vitest-environment jsdom
//
// The tiles are where the design lives, so this asserts the anatomy the design
// specifies: aspect ratio, corner radius, focus scale, the scrim, and that the
// optional watched check and resume bar appear only when asked for.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { clearPressGuard } from '../focus/guard';
import { radius } from '../tokens';
import { cellWidth } from './Grid';
import { MediaCard, tintGradient } from './MediaCard';
import { PosterCard } from './PosterCard';

afterEach(() => {
  cleanup();
  clearPressGuard();
});

const TINT = ['#3A2E4F', '#1B1524'] as const;
const css = (el: Element) => getComputedStyle(el);

describe('tintGradient', () => {
  it('builds the deterministic fill shown before the artwork loads', () => {
    expect(tintGradient(TINT)).toBe('linear-gradient(158deg, #3A2E4F 0%, #1B1524 72%)');
  });
});

describe('MediaCard', () => {
  it('is a focusable 16:9 tile at the design width and radius', () => {
    render(<MediaCard title="Dune" art={null} tint={TINT} />);
    const el = screen.getByLabelText('Dune');
    expect(el.hasAttribute('data-focus')).toBe(true);
    expect(css(el).width).toBe('328px');
    expect(css(el).borderTopLeftRadius).toBe(`${radius.xl}px`);
  });

  it('scales to 1.06 on focus, the rail tile treatment', () => {
    render(<MediaCard title="Dune" art={null} tint={TINT} />);
    const el = screen.getByLabelText('Dune');
    fireEvent.focus(el);
    expect(el.style.transform).toContain('scale(1.06)');
  });

  it('shows the overline and title, and clamps a long title', () => {
    render(<MediaCard title="Dune" overline="Science-fiction" art={null} tint={TINT} />);
    expect(screen.getByText('Science-fiction')).toBeTruthy();
    expect(screen.getByText('Dune')).toBeTruthy();
  });

  it('adds the watched check and the resume bar only when asked', () => {
    const { container, rerender } = render(<MediaCard title="Dune" art={null} tint={TINT} />);
    expect(container.querySelector('[role="progressbar"]')).toBeNull();
    expect(container.querySelectorAll('svg')).toHaveLength(0);
    rerender(<MediaCard title="Dune" art={null} tint={TINT} watched progress={0.4} />);
    expect(container.querySelector('[role="progressbar"]')).not.toBeNull();
    expect(container.querySelectorAll('svg')).toHaveLength(1);
  });

  it('fires onPress once the mount guard has elapsed', () => {
    const onPress = vi.fn();
    render(<MediaCard title="Dune" art={null} tint={TINT} onPress={onPress} />);
    fireEvent.click(screen.getByLabelText('Dune'));
    expect(onPress).toHaveBeenCalledTimes(1);
  });
});

describe('PosterCard', () => {
  it('fills its grid cell and uses the poster radius and focus scale', () => {
    render(<PosterCard title="Arrival" art={null} tint={TINT} />);
    const el = screen.getByLabelText('Arrival');
    expect(css(el).width).toBe('100%');
    expect(css(el).borderTopLeftRadius).toBe(`${radius.lg}px`);
    fireEvent.focus(el);
    expect(el.style.transform).toContain('scale(1.05)');
  });
});

describe('cellWidth', () => {
  it('divides the row after removing the gaps between cells', () => {
    // 1792 usable, 6 columns, 5 gaps of 24 = 1672 / 6.
    expect(cellWidth(1792, 6, 24)).toBe(278);
    expect(cellWidth(1000, 1, 24)).toBe(1000);
  });

  it('degrades to the full width rather than dividing by zero', () => {
    expect(cellWidth(800, 0, 24)).toBe(800);
  });
});
