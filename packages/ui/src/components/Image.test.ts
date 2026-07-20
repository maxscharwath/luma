// @vitest-environment jsdom
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { createElement } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Image } from './Image';

const el = (props: Record<string, unknown>) => createElement(Image, props);
const main = (root: HTMLElement) =>
  root.querySelector('img:not([aria-hidden])') as HTMLImageElement | null;
const under = (root: HTMLElement) =>
  root.querySelector('img[aria-hidden="true"]') as HTMLImageElement | null;

afterEach(cleanup);

describe('Image', () => {
  it('fades the artwork in on load (starts transparent, lazy + async)', () => {
    const { container } = render(el({ src: 'a.jpg' }));
    const img = main(container);
    if (!img) throw new Error('no <img> rendered');
    expect(img.style.opacity).toBe('0');
    expect(img.getAttribute('loading')).toBe('lazy');
    expect(img.getAttribute('decoding')).toBe('async');

    fireEvent.load(img);
    expect(img.style.opacity).toBe('1');
  });

  it('reveals the background (drops the image) when the source fails to load', () => {
    const { container } = render(el({ src: 'bad.jpg', background: 'red' }));
    const img = main(container);
    if (!img) throw new Error('no <img> rendered');
    fireEvent.error(img);
    expect(main(container)).toBeNull();
  });

  it('renders the fallback when there is no source, and after an error', () => {
    const fallback = createElement('span', { 'data-testid': 'fb' });
    const { container, rerender } = render(el({ src: null, fallback }));
    expect(main(container)).toBeNull();
    expect(container.querySelector('[data-testid="fb"]')).not.toBeNull();

    rerender(el({ src: 'bad.jpg', fallback }));
    const img = main(container);
    if (!img) throw new Error('no <img> rendered');
    fireEvent.error(img);
    expect(container.querySelector('[data-testid="fb"]')).not.toBeNull();
  });

  it('fill stretches the container to its positioned parent; default self-sizes', () => {
    const filled = render(el({ src: 'a.jpg', fill: true }));
    const box = filled.container.firstElementChild as HTMLElement;
    // Regression: an inline `position` must win, so `fill` (not a className)
    // is what stretches the box otherwise it collapses and the art reads black.
    expect(box.style.position).toBe('absolute');
    expect(box.style.right).toBe('0px');
    cleanup();

    const plain = render(el({ src: 'a.jpg' }));
    expect((plain.container.firstElementChild as HTMLElement).style.position).toBe('relative');
  });

  it('cross-fades on src change: holds the previous image underneath until the new one settles', () => {
    vi.useFakeTimers();
    try {
      const { container, rerender } = render(el({ src: 'a.jpg', duration: 100 }));
      const first = main(container);
      if (!first) throw new Error('no <img> rendered');
      fireEvent.load(first);

      rerender(el({ src: 'b.jpg', duration: 100 }));
      expect(under(container)?.getAttribute('src')).toBe('a.jpg');
      const next = main(container);
      if (!next) throw new Error('no incoming <img> rendered');
      expect(next.getAttribute('src')).toBe('b.jpg');
      expect(next.style.opacity).toBe('0');

      fireEvent.load(next);
      expect(next.style.opacity).toBe('1');

      // once the fade completes the stale underlay is removed
      act(() => vi.advanceTimersByTime(100));
      expect(under(container)).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
