import type { MediaItem } from '@luma/core';
import { type RefObject, useCallback, useEffect, useRef, useState } from 'react';
import type { MovieView } from '#web/shared/lib/api';

/** When no `credits` marker exists, surface the card this many seconds before
 * the end so series without markers still autoplay Netflix-style. */
const CREDITS_TAIL = 30;
/** Fixed countdown (seconds) shown in the card before auto-playing the next ep. */
const AUTO_NEXT = 12;

/**
 * Credits-aware "up next" orchestration for the player: decides when the card
 * shows (entering the `credits` marker, or the last {@link CREDITS_TAIL}s when
 * there's no marker), runs a fixed Netflix-style countdown that autoplays the
 * next episode at 0, and keeps the real `ended` event as a fallback. The
 * countdown only runs while the card is visible (not during a scrub) and the
 * user hasn't cancelled, so seeking near the end never teleports.
 */
export function useUpNext({
  item,
  next,
  onPlayNext,
  cur,
  dur,
  scrubbing,
  terminated,
  videoRef,
}: Readonly<{
  item: MovieView;
  next?: MediaItem | null;
  onPlayNext?: () => void;
  cur: number;
  dur: number;
  scrubbing: boolean;
  terminated: boolean;
  videoRef: RefObject<HTMLVideoElement | null>;
}>) {
  const [cancelled, setCancelled] = useState(false);
  const advancedRef = useRef(false);
  // Whether a next episode can be played at all (drives the manual ⏭ button).
  // NOT gated by `cancelled`: dismissing the auto-advance card must never remove
  // the manual skip control for the rest of the episode.
  const canAdvance = Boolean(next && onPlayNext);

  const credits = (item.markers ?? []).find((m) => m.kind === 'credits');
  // Without a credits marker, surface the card CREDITS_TAIL before the end but
  // only when the runtime is longer than that tail otherwise a short clip gets
  // creditsAt<=0 and the card (and its countdown) would show from cur=0.
  const creditsAt = credits ? credits.startMs / 1000 : dur - CREDITS_TAIL;
  const showUpNext =
    canAdvance && !cancelled && !terminated && !scrubbing && creditsAt > 0 && cur >= creditsAt;

  const advance = useCallback(() => {
    if (advancedRef.current) return;
    advancedRef.current = true;
    onPlayNext?.();
  }, [onPlayNext]);

  // Fixed countdown: (re)starts whenever the card becomes visible, ticks down to
  // 0, and clears when the card hides (scrub-out / cancel) or on unmount.
  const [countdown, setCountdown] = useState(AUTO_NEXT);
  useEffect(() => {
    setCountdown(AUTO_NEXT);
    if (!showUpNext) return;
    const id = setInterval(() => setCountdown((s) => Math.max(0, s - 1)), 1000);
    return () => clearInterval(id);
  }, [showUpNext]);

  // Reaching 0 autoplays, never a raw position check (which would fire mid-scrub).
  useEffect(() => {
    if (showUpNext && countdown === 0) advance();
  }, [showUpNext, countdown, advance]);

  // Real `ended` as a backup for streams that stop before the countdown elapses.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || !canAdvance) return;
    const onEnded = () => advance();
    v.addEventListener('ended', onEnded);
    return () => v.removeEventListener('ended', onEnded);
  }, [videoRef, canAdvance, advance]);

  return {
    showUpNext,
    countdown,
    total: AUTO_NEXT,
    canAdvance,
    advance,
    cancel: useCallback(() => setCancelled(true), []),
  };
}
