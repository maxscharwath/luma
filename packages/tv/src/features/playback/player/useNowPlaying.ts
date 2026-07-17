import type { KromaClient, MediaItem } from '@kroma/core';
import { useCallback, useEffect, useRef } from 'react';
import { getTauri } from '#tv/features/playback/player/engine';

/** Draw a poster blob (WebP / JPEG / the generated SVG) onto a canvas and re-encode as
 * JPEG - a raster the OS's NSImage decodes reliably (it can't render SVG at all). Returns
 * the JPEG bytes, or `null` if the image never loaded. */
function rasterize(blob: Blob, w = 342, h = 513): Promise<Uint8Array | null> {
  return new Promise((resolve) => {
    const url = URL.createObjectURL(blob);
    const img = new Image();
    const done = (out: Uint8Array | null) => {
      URL.revokeObjectURL(url);
      resolve(out);
    };
    img.onload = () => {
      try {
        const canvas = document.createElement('canvas');
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext('2d');
        if (!ctx) return done(null);
        ctx.drawImage(img, 0, 0, w, h);
        // JPEG, not PNG: a poster is a photo, so this is far smaller on the Tauri IPC
        // (the bytes cross as a number[]); NSImage decodes it just the same.
        canvas.toBlob(
          (jpeg) => {
            if (!jpeg) return done(null);
            jpeg
              .arrayBuffer()
              .then((buf) => done(new Uint8Array(buf)))
              .catch(() => done(null));
          },
          'image/jpeg',
          0.82,
        );
      } catch {
        done(null);
      }
    };
    img.onerror = () => done(null);
    img.src = url;
  });
}

/** Best-effort poster bytes for the OS widget: prefer the real cached art, falling back
 * to a full-item fetch if the passed item is a lightweight one missing its metadata. */
async function resolveArtwork(client: KromaClient, item: MediaItem): Promise<number[]> {
  try {
    const full = item.metadata?.posterUrl ? item : await client.item(item.id).catch(() => item);
    const blob = await client.posterBlob(full);
    const jpeg = await rasterize(blob);
    return jpeg ? Array.from(jpeg) : [];
  } catch {
    return [];
  }
}

/**
 * Push the current item + playback progress to the OS "Now Playing" widget (macOS
 * Control Center / the media-key HUD) via the native shell's `set_now_playing` command,
 * and honor its scrubber. Only active on the macOS libmpv shell (which registers the
 * command + MPRemoteCommandCenter); a no-op everywhere else. The poster is fetched +
 * rasterized on item change; play/pause just updates the rate + elapsed time.
 */
export function useNowPlaying(
  client: KromaClient,
  item: MediaItem,
  title: string,
  subtitle: string,
  durationSec: number,
  positionSec: number,
  playing: boolean,
  seekTo: (sec: number) => void,
): void {
  const bridge = getTauri();
  const active = !!bridge && '__KROMA_MPV__' in globalThis;

  // Current values in refs so the poster/seek effects don't re-run on every position tick
  // (they only care about item changes / play-pause), yet always send fresh data.
  const infoRef = useRef({ title, subtitle, durationSec, positionSec, playing });
  infoRef.current = { title, subtitle, durationSec, positionSec, playing };
  const seekRef = useRef(seekTo);
  seekRef.current = seekTo;

  // One place to build + send the payload (artwork empty = keep the current poster).
  const push = useCallback(
    (artwork: number[]) => {
      if (!bridge) return;
      const info = infoRef.current;
      void bridge.core
        .invoke('set_now_playing', {
          title: info.title,
          artist: info.subtitle,
          duration: info.durationSec,
          position: info.positionSec,
          playing: info.playing,
          artwork,
        })
        .catch(() => undefined);
    },
    [bridge],
  );

  // Dragging the OS scrubber (Control Center) fires `media-seek` with a target second.
  useEffect(() => {
    if (!active || !bridge) return;
    let un: (() => void) | undefined;
    let dead = false;
    void bridge.event
      .listen('media-seek', (e) => {
        const pos = Number((e as { payload?: unknown }).payload ?? Number.NaN);
        if (Number.isFinite(pos)) seekRef.current(pos);
      })
      .then((u) => {
        if (dead) u();
        else un = u;
      });
    return () => {
      dead = true;
      un?.();
    };
  }, [active, bridge]);

  // On item change ONLY: resolve + rasterize the poster, then push the full info.
  // biome-ignore lint/correctness/useExhaustiveDependencies: key on item.id (not identity); title/subtitle/duration are read via ref in push.
  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    void resolveArtwork(client, item).then((artwork) => {
      if (!cancelled) push(artwork);
    });
    return () => {
      cancelled = true;
    };
  }, [active, client, item.id, push]);

  // On play/pause: update the rate + elapsed time (keep the current poster).
  // biome-ignore lint/correctness/useExhaustiveDependencies: fire on play/pause; the value is read via ref in push.
  useEffect(() => {
    if (active) push([]);
  }, [active, playing, push]);
}
