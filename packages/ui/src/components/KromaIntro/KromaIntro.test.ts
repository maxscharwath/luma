// @vitest-environment jsdom
import { act, cleanup, render } from '@testing-library/react';
import { createElement } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EXIT_MS } from './constants';
import { KromaIntro } from './index';

const play = vi.fn<() => Promise<void>>();

/** jsdom has no media stack: back `play`/`pause` and the two properties the
 * intro reads with plain stubs so the timeline is fully driveable from a test. */
function stubMedia() {
  const proto = HTMLMediaElement.prototype;
  for (const [key, value] of [
    ['play', play],
    ['pause', () => undefined],
    ['currentTime', 0],
    ['duration', 5],
  ] as const) {
    Object.defineProperty(proto, key, { configurable: true, writable: true, value });
  }
}

/** Mount the intro and settle the autoplay promise chain. */
async function mount(onDone: () => void) {
  const view = render(createElement(KromaIntro, { onDone }));
  await act(async () => undefined);
  const video = view.container.querySelector('video');
  if (!video) throw new Error('no <video> rendered');
  return { ...view, video };
}

function press(key: string) {
  act(() => {
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
  });
}

describe('KromaIntro (film path)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    stubMedia();
    // Sound-first is blocked (as in Chrome), the muted retry succeeds: the film
    // then runs muted for its whole length, which is the normal web path.
    play.mockReset();
    play.mockRejectedValueOnce(new Error('NotAllowedError')).mockResolvedValue(undefined);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it('falls back to muted playback when sound-first autoplay is blocked', async () => {
    const { video } = await mount(vi.fn());
    expect(play).toHaveBeenCalledTimes(2);
    expect(video.muted).toBe(true);
  });

  it('a gesture at the very top restarts the film with sound', async () => {
    const { video } = await mount(vi.fn());
    video.currentTime = 0.2;

    act(() => {
      document.dispatchEvent(new Event('pointerdown'));
    });
    expect(video.muted).toBe(false);
    expect(video.currentTime).toBe(0);
    expect(play).toHaveBeenCalledTimes(3);
  });

  it('a gesture mid-film only unmutes: no click or stray key restarts it', async () => {
    const { video } = await mount(vi.fn());
    video.currentTime = 2.5;

    act(() => {
      document.dispatchEvent(new Event('pointerdown'));
    });
    press('ArrowRight');

    expect(video.muted).toBe(false);
    expect(video.currentTime).toBe(2.5);
    expect(play).toHaveBeenCalledTimes(2); // still the two mount attempts
  });

  it('ignores a video error that lands after a skip (no second intro)', async () => {
    const onDone = vi.fn();
    const { video, container } = await mount(onDone);

    press('Enter'); // skip: the hand-off is now pending
    act(() => {
      video.dispatchEvent(new Event('error'));
    });
    act(() => vi.advanceTimersByTime(EXIT_MS));

    expect(onDone).toHaveBeenCalledTimes(1);
    // the CSS fallback never took over (it renders no <video>)
    expect(container.querySelector('video')).not.toBeNull();
  });

  it('swaps in the CSS fallback when playback genuinely fails', async () => {
    const { video, container } = await mount(vi.fn());
    await act(async () => {
      video.dispatchEvent(new Event('error'));
    });
    expect(container.querySelector('video')).toBeNull();
  });
});
