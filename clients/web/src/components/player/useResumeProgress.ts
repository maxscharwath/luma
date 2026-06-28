import { useCallback, useEffect, useState } from 'react';
import type { MovieView } from '#web/lib/api';
import { useAuth } from '#web/lib/auth';

export interface ResumeProgress {
  /** Saved position (seconds) to resume from, or null. */
  resumeAt: number | null;
  /** Whether the "resumed at …" toast is showing. */
  showResume: boolean;
  setShowResume: (v: boolean) => void;
}

/**
 * Per-user resume + progress persistence for the player. Fetches the saved
 * position, seeks to it once the media is ready (flashing a toast), and writes
 * progress every 10 s / on pause / on close — clearing it once ~finished.
 */
export function useResumeProgress(
  videoRef: React.RefObject<HTMLVideoElement>,
  item: MovieView,
  // Offset-aware position control from useVideoPlayback: `seekTo` an absolute
  // second (re-`-ss`-es the seamless stream so resume is instantly available),
  // `getPosition` reads the absolute current position. Falls back to the raw
  // <video> timeline when omitted (single-stream direct-play).
  position?: { seekTo: (absSec: number) => void; getPosition: () => number },
): ResumeProgress {
  const { client, user } = useAuth();
  const [resumeAt, setResumeAt] = useState<number | null>(null);
  const [showResume, setShowResume] = useState(false);

  // Fetch the saved position for this item (per-user) → offer to resume.
  useEffect(() => {
    if (!user) return;
    let cancelled = false;
    client
      .itemProgress(item.id)
      .then((p) => {
        if (cancelled || !p) return;
        const durMs = p.durationMs ?? item.durationMs ?? 0;
        const posSec = p.positionMs / 1000;
        if (posSec > 15 && (!durMs || p.positionMs < durMs * 0.95)) setResumeAt(posSec);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, user, item.id, item.durationMs]);

  // Seek to the saved position once the media is ready, and flash a toast.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || resumeAt == null) return;
    let applied = false;
    const apply = () => {
      if (applied) return;
      applied = true;
      // Seamless: re-`-ss` the stream at resumeAt (instantly available). Direct:
      // a normal range seek. Either way we land at the real saved position.
      if (position) position.seekTo(resumeAt);
      else if (v.currentTime < resumeAt - 2) v.currentTime = resumeAt;
      setShowResume(true);
    };
    if (v.readyState >= 1) apply();
    else v.addEventListener('loadedmetadata', apply, { once: true });
    const hide = setTimeout(() => setShowResume(false), 6000);
    return () => {
      clearTimeout(hide);
      v.removeEventListener('loadedmetadata', apply);
    };
    // `position` is read once when resumeAt resolves — excluded so a new
    // position identity can't retrigger the resume seek.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [videoRef, resumeAt]);

  // Persist progress: every 10 s while watching, on pause, on close/unmount, and
  // clear it once the item is ~finished (drops it from "Reprendre").
  const saveProgress = useCallback(() => {
    const v = videoRef.current;
    if (!v || !user) return;
    // ABSOLUTE position + catalogue runtime — the seamless stream's own
    // currentTime/duration is relative to the -ss offset, so never use them here.
    const pos = position ? position.getPosition() : v.currentTime;
    const durSec = item.durationMs ? item.durationMs / 1000 : v.duration;
    if (!Number.isFinite(durSec) || durSec <= 0 || pos < 5) return;
    if (pos > durSec * 0.97) void client.deleteProgress(item.id);
    else void client.saveProgress(item.id, pos * 1000, durSec * 1000);
  }, [videoRef, client, user, item.id, item.durationMs, position]);

  useEffect(() => {
    if (!user) return;
    const v = videoRef.current;
    const interval = setInterval(saveProgress, 10000);
    const onUnload = () => saveProgress();
    const onEnded = () => void client.deleteProgress(item.id);
    window.addEventListener('beforeunload', onUnload);
    v?.addEventListener('pause', saveProgress);
    v?.addEventListener('ended', onEnded);
    return () => {
      clearInterval(interval);
      window.removeEventListener('beforeunload', onUnload);
      v?.removeEventListener('pause', saveProgress);
      v?.removeEventListener('ended', onEnded);
      saveProgress(); // final save when leaving the player
    };
  }, [videoRef, user, saveProgress, client, item.id]);

  return { resumeAt, showResume, setShowResume };
}
