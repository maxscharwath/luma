import type { KromaClient, MediaItem } from '@kroma/core';
import { useCallback, useEffect, useRef } from 'react';

/** The slice of the engine resume/persistence needs, so this works the same for
 * the HTML `<video>` and native AVPlay backends. */
export interface PersistPort {
  getPosition: () => number;
  getDuration: () => number;
  /** True while playback is paused (persist on pause). */
  paused: boolean;
  /** Increments when playback reaches the end (mark watched). */
  endedNonce: number;
}

/**
 * Progress persistence for the TV player, engine-agnostic: persists progress every
 * 10 s, on pause, on ~finish, and on exit (cleanup). The RESUME position itself is now
 * applied by `useDirectPlayback` as the engine's `startSec` (so it opens directly at
 * the resume point), not re-seeked here.
 */
export function useResumeAndPersist(client: KromaClient, item: MediaItem, port: PersistPort): void {
  const portRef = useRef(port);
  portRef.current = port;

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
