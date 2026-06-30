import type { LumaClient, MediaItem } from '@luma/core';
import { useCallback, useEffect, useRef } from 'react';

/** The slice of the engine resume/persistence needs, so this works the same for
 * the HTML `<video>` and native AVPlay backends. */
export interface PersistPort {
  getPosition: () => number;
  getDuration: () => number;
  seekTo: (sec: number) => void;
  /** True once the engine can play (apply the resume seek). */
  ready: boolean;
  /** True while playback is paused (persist on pause). */
  paused: boolean;
  /** Increments when playback reaches the end (mark watched). */
  endedNonce: number;
}

/**
 * Resume + progress persistence for the TV player, engine-agnostic: restores the
 * saved resume position once the engine is ready, then persists progress every
 * 10 s, on pause, on ~finish, and on exit (cleanup).
 */
export function useResumeAndPersist(client: LumaClient, item: MediaItem, port: PersistPort): void {
  const portRef = useRef(port);
  portRef.current = port;
  const appliedRef = useRef(false);

  // Reset the applied-once guard when the item changes (a new title may resume).
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset only on item change.
  useEffect(() => {
    appliedRef.current = false;
  }, [item]);

  useEffect(() => {
    if (!port.ready || !client.hasAuth || appliedRef.current) return;
    let cancelled = false;
    client
      .itemProgress(item.id)
      .then((p) => {
        if (cancelled || !p || appliedRef.current) return;
        const durMs = p.durationMs ?? item.durationMs ?? 0;
        const posSec = p.positionMs / 1000;
        if (posSec > 15 && (!durMs || p.positionMs < durMs * 0.95)) {
          appliedRef.current = true;
          if (portRef.current.getPosition() < posSec - 2) portRef.current.seekTo(posSec);
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, item, port.ready]);

  const save = useCallback(() => {
    if (!client.hasAuth) return;
    const p = portRef.current;
    const d = p.getDuration();
    const pos = p.getPosition();
    if (!Number.isFinite(d) || d <= 0 || pos < 5) return;
    // ~Finished → mark watched (clears the resume position server-side too).
    if (pos > d * 0.97) void client.markWatched(item.id);
    else void client.saveProgress(item.id, pos * 1000, d * 1000);
  }, [client, item]);

  // Persist every 10 s, and once more on unmount.
  useEffect(() => {
    if (!client.hasAuth) return;
    const interval = setInterval(save, 10000);
    return () => {
      clearInterval(interval);
      save();
    };
  }, [client, save]);

  // Persist on pause.
  useEffect(() => {
    if (port.paused) save();
  }, [port.paused, save]);

  // Mark watched the moment playback reaches the end.
  useEffect(() => {
    if (port.endedNonce > 0 && client.hasAuth) void client.markWatched(item.id);
  }, [port.endedNonce, client, item]);
}
