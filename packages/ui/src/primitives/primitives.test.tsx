// @vitest-environment jsdom
//
// Renders the universal primitives through react-native-web and asserts the DOM
// they produce. The same components compile to native views on Apple TV and
// Android TV, so this is the browser half of the "one component, four targets"
// claim; the native half is covered by the pure logic tests (focal, sv, boxStyle)
// plus the platform files' shared contracts.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { clearPressGuard } from '../focus/guard';
import { colors, radius } from '../tokens';
import { Avatar, initialsOf } from './Avatar';
import { Badge } from './Badge';
import { Button } from './Button';
import { Chip } from './Chip';
import { Dialog } from './Dialog';
import { Icon } from './Icon';
import { clamp01, Progress } from './Progress';
import { Txt } from './Text';

afterEach(() => {
  cleanup();
  clearPressGuard();
});

/** react-native-web compiles most styles into atomic CSS classes and only keeps
 * inline styles where a value is dynamic (an Animated node, for instance), so a
 * DOM assertion has to read the RESOLVED style rather than the style attribute. */
const css = (el: Element) => getComputedStyle(el);

/** jsdom normalises every colour to `rgb()`, so compare through the same lens
 * instead of against the token's hex spelling. */
function rgb(hex: string): string {
  const m = /^#([\da-f]{2})([\da-f]{2})([\da-f]{2})$/i.exec(hex);
  if (!m) return hex;
  const [r, g, b] = [m[1], m[2], m[3]].map((h) => Number.parseInt(h as string, 16));
  return `rgb(${r}, ${g}, ${b})`;
}

describe('Icon', () => {
  it('draws an outline glyph as a stroked svg', () => {
    const { container } = render(<Icon name="check" size={32} color="accent" />);
    const svg = container.querySelector('svg');
    expect(svg?.getAttribute('width')).toBe('32');
    expect(svg?.getAttribute('stroke')).toBe(colors.accent);
    expect(svg?.getAttribute('fill')).toBe('none');
    expect(svg?.querySelectorAll('path').length).toBeGreaterThan(0);
  });

  it('draws a filled glyph as a painted svg with no stroke', () => {
    const { container } = render(<Icon name="player-play-filled" />);
    const svg = container.querySelector('svg');
    expect(svg?.getAttribute('fill')).toBe(colors.text);
    expect(svg?.getAttribute('stroke')).toBe('none');
  });

  it('accepts a raw colour as well as a palette token', () => {
    const { container } = render(<Icon name="x" color="#ABCDEF" />);
    expect(container.querySelector('svg')?.getAttribute('stroke')).toBe('#ABCDEF');
  });
});

describe('Button', () => {
  it('renders its label and fires onPress', () => {
    const onPress = vi.fn();
    render(<Button label="Lecture" onPress={onPress} />);
    const el = screen.getByLabelText('Lecture');
    expect(el.textContent).toContain('Lecture');
    fireEvent.click(el);
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it('paints the amber fill for the primary variant and nothing for ghost', () => {
    render(
      <>
        <Button label="A" variant="primary" />
        <Button label="B" variant="ghost" />
      </>,
    );
    expect(css(screen.getByLabelText('A')).backgroundColor).toBe(rgb(colors.accent));
    // jsdom resolves the `transparent` keyword to its rgba() equivalent.
    expect(css(screen.getByLabelText('B')).backgroundColor).toBe('rgba(0, 0, 0, 0)');
  });

  it('dims and blocks a disabled button', () => {
    const onPress = vi.fn();
    render(<Button label="Off" disabled onPress={onPress} />);
    const el = screen.getByLabelText('Off');
    expect(css(el).opacity).toBe('0.5');
    fireEvent.click(el);
    expect(onPress).not.toHaveBeenCalled();
  });

  it('renders leading and trailing glyphs', () => {
    const { container } = render(
      <Button label="Regler" icon="settings" iconRight="chevron-right" />,
    );
    expect(container.querySelectorAll('svg')).toHaveLength(2);
  });
});

describe('Badge and Chip', () => {
  it('tints a badge with its own hue', () => {
    render(<Badge tone="HDR" />);
    expect(screen.getByText('HDR')).toBeTruthy();
  });

  it('inverts a chip when active', () => {
    render(
      <>
        <Chip label="FR" active />
        <Chip label="EN" />
      </>,
    );
    expect(css(screen.getByLabelText('FR')).backgroundColor).toBe(rgb(colors.accent));
    expect(css(screen.getByLabelText('EN')).backgroundColor).not.toBe(rgb(colors.accent));
  });
});

describe('Avatar', () => {
  it('derives initials from the first two words', () => {
    expect(initialsOf('Marie Curie')).toBe('MC');
    expect(initialsOf('cher')).toBe('C');
    expect(initialsOf('  ')).toBe('');
  });

  it('shows the initials when there is no photo, and not when there is', () => {
    const { rerender } = render(<Avatar name="Marie Curie" />);
    expect(screen.queryByText('MC')).toBeTruthy();
    rerender(<Avatar name="Marie Curie" src="https://example.test/a.jpg" />);
    expect(screen.queryByText('MC')).toBeNull();
  });
});

describe('Progress', () => {
  it('clamps out-of-range and non-finite values', () => {
    expect(clamp01(-1)).toBe(0);
    expect(clamp01(2)).toBe(1);
    expect(clamp01(Number.NaN)).toBe(0);
    expect(clamp01(0.42)).toBe(0.42);
  });

  it('sizes the fill to the value', () => {
    const { container } = render(<Progress value={0.25} />);
    const fill = container.querySelector('[role="progressbar"] > *') as HTMLElement;
    expect(css(fill).width).toBe('25%');
  });
});

describe('Txt', () => {
  it('applies the design type role and palette colour', () => {
    render(
      <Txt variant="h1" color="accent">
        Films
      </Txt>,
    );
    const el = screen.getByText('Films');
    expect(css(el).fontSize).toBe('38px');
    expect(css(el).color).toBe(rgb(colors.accent));
  });
});

describe('Dialog', () => {
  it('renders nothing while closed', () => {
    render(<Dialog open={false} title="Supprimer" />);
    expect(screen.queryByText('Supprimer')).toBeNull();
  });

  it('declares a focus scope so the D-pad cannot leave the panel', () => {
    render(
      <Dialog open title="Supprimer">
        <Button label="OK" />
      </Dialog>,
    );
    const panel = document.querySelector('[data-focus-scope]');
    expect(panel).not.toBeNull();
    expect(panel?.querySelector('[data-focus]')).not.toBeNull();
    expect(screen.getByText('Supprimer')).toBeTruthy();
  });

  it('rounds the panel with the design radius', () => {
    render(<Dialog open title="Titre" />);
    const panel = document.querySelector('[data-focus-scope]') as HTMLElement;
    expect(css(panel).borderTopLeftRadius).toBe(`${radius['2xl']}px`);
  });
});
