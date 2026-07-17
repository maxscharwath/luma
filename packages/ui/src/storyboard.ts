// Shared scrub-bar "storyboard" hook, behind each client's seek preview. Loads an
// item's single sprite sheet of evenly-spaced thumbnails for the YouTube-style
// hover / 10-foot scrub preview. The server builds the sheet lazily, so this polls
// while it is `pending`, then preloads the image so the first hover/scrub paints
// instantly. Per-position cost is a CSS `background-position` lookup: no canvas,
// no per-frame decode.
//
// The client is injected so the web (global `kromaClient()`) and the TV (its auth
// client) can share the exact same logic. `generate: false` (dashboard thumbs)
// does a single fetch and never polls/awaits lazy generation, so it cannot compete
// with live-playback IO.

import type { KromaClient, StoryboardManifest } from '@kroma/core';
import { useCallback, useEffect, useState } from 'react';

/** CSS for one preview tile, scaled to a requested display width. Spread onto a
 * fixed-size box (the sheet shows through via `background-position`). */
export interface StoryboardTile {
  width: number;
  height: number;
  backgroundImage: string;
  backgroundPosition: string;
  backgroundSize: string;
  backgroundRepeat: 'no-repeat';
}

export interface Storyboard {
  /** True once the manifest is resolved AND the sprite sheet has finished loading. */
  ready: boolean;
  /** CSS for the tile at `sec`, scaled to `displayW` px wide; null until ready. */
  tile: (sec: number, displayW: number) => StoryboardTile | null;
}

const POLL_MS = 1500;
const FAST_POLLS = 40; // ~60 s of fast polling while ffmpeg builds the sheet
const SLOW_MS = 15000; // then back off, so a late finish on a slow NAS is still caught
const MAX_TRIES = FAST_POLLS + 240; // overall bound (~1 h) so we never dead-stop early

/**
 * Loads an item's scrub-bar storyboard for the seek preview. Polls while the sheet
 * is `pending`, then preloads it so the first hover/scrub never flashes an empty or
 * half-loaded sheet.
 *
 * `generate: false` (dashboard thumbnails) does a single fetch and never
 * polls/awaits lazy generation, so it can't compete with live-playback IO.
 */
export function useStoryboard(
  client: KromaClient,
  itemId: string,
  { generate = true }: { generate?: boolean } = {},
): Storyboard {
  const [manifest, setManifest] = useState<StoryboardManifest | null>(null);
  const [sheetUrl, setSheetUrl] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let resolved = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let tries = 0;
    setManifest(null);
    setSheetUrl(null);
    setLoaded(false);

    const poll = () => {
      client
        .storyboard(itemId)
        .then((res) => {
          if (cancelled || resolved) return;
          if (res === 'pending') {
            if (!generate) return; // dashboard thumbs never kick/await generation
            tries += 1;
            // Fast poll for the first ~60 s, then slow poll (never a dead stop).
            const delay = tries <= FAST_POLLS ? POLL_MS : SLOW_MS;
            if (tries <= MAX_TRIES) timer = setTimeout(poll, delay);
            return;
          }
          if (!res) return; // no usable file/duration: silently fall back to the time label
          resolved = true;
          const url = client.resolveArt(res.url) ?? res.url;
          setManifest(res);
          setSheetUrl(url);
          // Preload so the first hover never flashes an empty/half-loaded sheet.
          const img = new Image();
          img.onload = () => {
            if (!cancelled) setLoaded(true);
          };
          img.src = url;
        })
        .catch(() => undefined);
    };
    poll();

    // Re-check when the tab becomes visible again, so a sheet that finished while
    // backgrounded is picked up without a tight interval. (Only for the real
    // player: dashboard thumbs stay a single fetch.)
    const onVisible = () => {
      if (document.visibilityState !== 'visible' || cancelled || resolved) return;
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
      tries = 0;
      poll();
    };
    if (generate) document.addEventListener('visibilitychange', onVisible);

    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      if (generate) document.removeEventListener('visibilitychange', onVisible);
    };
  }, [client, itemId, generate]);

  const tile = useCallback(
    (sec: number, displayW: number): StoryboardTile | null => {
      if (!manifest || !sheetUrl || !loaded) return null;
      const { interval, tileW, tileH, cols, rows, count } = manifest;
      const idx = Math.max(0, Math.min(count - 1, Math.floor(sec / interval)));
      const col = idx % cols;
      const row = Math.floor(idx / cols);
      const scale = displayW / tileW;
      return {
        width: displayW,
        height: Math.round(tileH * scale),
        backgroundImage: `url("${sheetUrl}")`,
        backgroundPosition: `-${Math.round(col * tileW * scale)}px -${Math.round(row * tileH * scale)}px`,
        backgroundSize: `${Math.round(cols * tileW * scale)}px ${Math.round(rows * tileH * scale)}px`,
        backgroundRepeat: 'no-repeat',
      };
    },
    [manifest, sheetUrl, loaded],
  );

  return { ready: loaded && manifest != null, tile };
}
