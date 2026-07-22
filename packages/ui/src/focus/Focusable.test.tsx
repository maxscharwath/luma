// @vitest-environment jsdom
//
// Proves the universal kit actually renders through react-native-web: the same
// <Focusable> source that Apple TV compiles natively must produce a focusable
// DOM node carrying `data-focus`, the amber ring while focused, and it must obey
// the OK guard. If this file passes, the Tizen / webOS bundles have a working
// view layer.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { colors, ring } from '../tokens';
import { Focusable } from './Focusable';
import { armPressGuard, clearPressGuard } from './guard';

afterEach(() => {
  cleanup();
  clearPressGuard();
});

/** The rendered host element for a focusable labelled `label`. */
const host = (label: string) => screen.getByLabelText(label);

/** A remote OK press.
 *
 * `accessibilityRole="button"` makes react-native-web render a real <button>,
 * and for natively interactive elements it deliberately does NOT synthesise the
 * press from keydown/keyup: it lets the browser do what browsers do, which is
 * fire a `click` when Enter lands on a focused button. That is the same path the
 * pre-uikit Tizen / webOS app used, and it is why the web navigation engine does
 * not handle Enter itself.
 *
 * jsdom implements neither the activation behaviour nor `click()` from a key, so
 * the test stands in for the browser with an explicit click. The keydown/keyup
 * pair is still sent so the assertion covers the real event sequence. */
function pressOk(el: HTMLElement) {
  fireEvent.keyDown(el, { key: 'Enter' });
  fireEvent.keyUp(el, { key: 'Enter' });
  fireEvent.click(el);
}

describe('Focusable on react-native-web', () => {
  it('renders a keyboard-reachable host tagged for the spatial navigator', () => {
    render(<Focusable label="Lecture" />);
    const el = host('Lecture');
    expect(el.getAttribute('tabindex')).toBe('0');
    expect(el.hasAttribute('data-focus')).toBe(true);
    expect(el.getAttribute('role')).toBe('button');
  });

  it('marks the screen entry point so focusFirst can find it', () => {
    render(
      <>
        <Focusable label="Premier" />
        <Focusable label="Entree" autoFocus />
      </>,
    );
    expect(host('Premier').hasAttribute('data-autofocus')).toBe(false);
    expect(host('Entree').hasAttribute('data-autofocus')).toBe(true);
  });

  it('takes the focusable out of the tab order when disabled', () => {
    render(<Focusable label="Indispo" disabled />);
    expect(host('Indispo').getAttribute('tabindex')).toBe('-1');
    expect(host('Indispo').hasAttribute('data-focus')).toBe(false);
  });

  it('paints the amber ring only while focused', () => {
    render(<Focusable label="Carte" />);
    const el = host('Carte');
    expect(el.style.boxShadow).toBe('');
    fireEvent.focus(el);
    expect(el.style.boxShadow.replace(/\s+/g, ' ')).toBe(ring.focusLift);
    fireEvent.blur(el);
    expect(el.style.boxShadow).toBe('');
  });

  it('scales on focus when the design asks for it, and not otherwise', () => {
    render(
      <>
        <Focusable label="Tuile" focusScale={1.06} />
        <Focusable label="Plate" />
      </>,
    );
    fireEvent.focus(host('Tuile'));
    expect(host('Tuile').style.transform).toContain('scale(1.06)');
    fireEvent.focus(host('Plate'));
    expect(host('Plate').style.transform).toBe('');
  });

  it('fires onPress on Enter, the key a TV remote OK sends', () => {
    const onPress = vi.fn();
    render(<Focusable label="OK" onPress={onPress} />);
    pressOk(host('OK'));
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it('swallows the press that carried over from the previous screen', () => {
    const onPress = vi.fn();
    render(<Focusable label="OK" onPress={onPress} />);
    armPressGuard();
    pressOk(host('OK'));
    expect(onPress).not.toHaveBeenCalled();
    clearPressGuard();
    pressOk(host('OK'));
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it('exposes focus state to a render-prop child', () => {
    render(
      <Focusable label="Etat">
        {({ focused }) => <span data-testid="state">{focused ? 'on' : 'off'}</span>}
      </Focusable>,
    );
    expect(screen.getByTestId('state').textContent).toBe('off');
    fireEvent.focus(host('Etat'));
    expect(screen.getByTestId('state').textContent).toBe('on');
  });

  it('applies focusedStyle from the design tokens', () => {
    render(<Focusable label="Chip" focusedStyle={{ backgroundColor: colors.accentSoft }} />);
    const el = host('Chip');
    fireEvent.focus(el);
    expect(el.style.backgroundColor.replace(/\s+/g, ' ')).toBe(colors.accentSoft);
  });
});
