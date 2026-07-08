import { useEffect } from 'react';
import type { MovieView } from '#web/shared/lib/api';

/** Global keyboard shortcuts for the player (play/pause, ±skip, volume, mute,
 * fullscreen, stats, captions, number-key seek, Escape). The player's input
 * wiring, pulled out of `Player.tsx`. */
export function usePlayerHotkeys(
  o: Readonly<{
    videoRef: React.RefObject<HTMLVideoElement>;
    togglePlay: () => void;
    skip: (delta: number) => void;
    setVol: (val: number) => void;
    toggleMute: () => void;
    toggleFullscreen: () => void;
    seekTo: (absSec: number) => void;
    dur: number;
    pickSub: (index: number | null) => void;
    activeSub: number | null;
    subs: MovieView['subs'];
    avOpen: boolean;
    setAvOpen: (open: boolean) => void;
    setStatsOpen: React.Dispatch<React.SetStateAction<boolean>>;
    onClose: () => void;
    poke: () => void;
    /** Admin stopped the stream: swallow every transport key so the viewer can't
     * silently resume an untracked stream; only Escape (close) is honored. */
    locked?: boolean;
  }>,
): void {
  const {
    videoRef,
    togglePlay,
    skip,
    setVol,
    toggleMute,
    toggleFullscreen,
    seekTo,
    dur,
    pickSub,
    activeSub,
    subs,
    avOpen,
    setAvOpen,
    setStatsOpen,
    onClose,
    poke,
    locked = false,
  } = o;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      // Stream terminated by an admin: lock the transport, only Escape exits.
      if (locked) {
        e.preventDefault();
        if (e.key === 'Escape') onClose();
        return;
      }
      switch (e.key) {
        case ' ':
        case 'k':
          e.preventDefault();
          togglePlay();
          break;
        case 'ArrowLeft':
          skip(-5);
          break;
        case 'ArrowRight':
          skip(5);
          break;
        case 'j':
          skip(-10);
          break;
        case 'l':
          skip(10);
          break;
        case 'ArrowUp':
          e.preventDefault();
          setVol((videoRef.current?.volume ?? 1) + 0.05);
          break;
        case 'ArrowDown':
          e.preventDefault();
          setVol((videoRef.current?.volume ?? 1) - 0.05);
          break;
        case 'm':
          toggleMute();
          break;
        case 'f':
          toggleFullscreen();
          break;
        case 'i':
          setStatsOpen((s) => !s);
          break;
        case 'c':
          pickSub(activeSub == null ? (subs.find((s) => s.url)?.index ?? null) : null);
          break;
        case 'Escape':
          if (avOpen) setAvOpen(false);
          else if (document.fullscreenElement) void document.exitFullscreen();
          else onClose();
          break;
        default:
          // Number keys → jump to N/10 of the movie (offset-aware via seekTo).
          if (/^[0-9]$/.test(e.key) && dur) {
            seekTo((Number(e.key) / 10) * dur);
          }
      }
      poke();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    videoRef,
    togglePlay,
    skip,
    setVol,
    toggleMute,
    toggleFullscreen,
    pickSub,
    activeSub,
    subs,
    avOpen,
    onClose,
    poke,
    seekTo,
    dur,
    locked,
  ]);
}
