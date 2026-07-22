// @vitest-environment jsdom
//
// The first screen piece moved onto the universal kit. It renders through
// react-native-web here exactly as it does in the Tizen / webOS bundles, and
// compiles to native views on Apple TV / Android TV from the same source.

import { clearPressGuard } from '@kroma/ui/kit';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { type GridCard, TvGrid } from '#tv/features/catalog/home/TvGrid';

afterEach(() => {
  cleanup();
  clearPressGuard();
});

function cards(n: number, over: Partial<GridCard> = {}): GridCard[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `id-${i}`,
    title: `Film ${i}`,
    poster: `/art/${i}.jpg`,
    colors: ['#3A2E4F', '#1B1524'] as [string, string],
    onClick: () => {},
    ...over,
  }));
}

describe('TvGrid', () => {
  it('renders one focusable poster per card', () => {
    const { container } = render(<TvGrid cards={cards(8)} />);
    expect(container.querySelectorAll('[data-focus]')).toHaveLength(8);
    expect(screen.getByLabelText('Film 0')).toBeTruthy();
  });

  it('renders only the first chunk of a long library', () => {
    const { container } = render(<TvGrid cards={cards(500)} />);
    expect(container.querySelectorAll('[data-focus]')).toHaveLength(120);
  });

  it('activates a tile on press', () => {
    const onClick = vi.fn();
    render(<TvGrid cards={cards(1, { onClick })} />);
    fireEvent.click(screen.getByLabelText('Film 0'));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('converts the percentage progress the card carries into a 0..1 bar', () => {
    const { container } = render(<TvGrid cards={cards(1, { progress: 40 })} />);
    const fill = container.querySelector('[role="progressbar"] > *') as HTMLElement;
    expect(getComputedStyle(fill).width).toBe('40%');
  });

  it('lays the tiles out at the design column width', () => {
    const { container } = render(<TvGrid cards={cards(3)} />);
    // 1792px of content, 8 columns, 24px gaps -> 203px cells.
    const cell = container.querySelector('[data-focus]')?.parentElement as HTMLElement;
    expect(getComputedStyle(cell).width).toBe('203px');
  });
});
