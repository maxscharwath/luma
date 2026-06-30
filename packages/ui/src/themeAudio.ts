import { useCallback, useEffect, useRef, useState } from 'react';

/** Persisted across detail pages + reloads (per device). */
const MUTE_KEY = 'luma.theme.muted';
/** Quiet background level present but never competing with the user. */
const TARGET_VOLUME = 0.35;
const FADE_IN_MS = 900;
const FADE_OUT_MS = 600;

/** Read the persisted mute preference; safe on the server / in private mode. */
function readMuted(): boolean {
  try {
    return localStorage.getItem(MUTE_KEY) === '1';
  } catch {
    return false;
  }
}

/** Fade a (detached) audio element to silence over `ms`, then pause it owning a
 * private interval so it always completes. Used on unmount/theme change: the
 * in-component `fadeTo` shares one ref, so the *next* theme's fade-in would clear
 * a shared fade-out before it paused the previous element, leaving it looping. */
function fadeOutAndStop(a: HTMLAudioElement, ms: number): void {
  const steps = Math.max(1, Math.round(ms / 50));
  const from = a.volume;
  let i = 0;
  const id = setInterval(() => {
    i += 1;
    a.volume = Math.max(0, from * (1 - i / steps));
    if (i >= steps) {
      clearInterval(id);
      a.pause();
    }
  }, 50);
}

export interface ThemeAudio {
  /** Whether a theme is available gates whether the mute toggle renders. */
  active: boolean;
  muted: boolean;
  toggle: () => void;
}

/**
 * Plex-style theme playback for a detail page: loops `themeUrl` at a low volume,
 * fading in once it can play and fading out + stopping on unmount (i.e. when the
 * user hits Play or navigates away).
 *
 * Browsers gate autoplay-with-sound behind a user gesture arriving on this page
 * via a click usually satisfies that, and a one-shot pointer/key fallback covers
 * the rest. The mute preference lives in localStorage so it persists across
 * pages; React state mirrors it only for the toggle icon (kept SSR-safe by
 * starting unmuted and syncing on mount).
 */
export function useThemeAudio(themeUrl: string | null | undefined): ThemeAudio {
  const [muted, setMuted] = useState(false);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const fadeRef = useRef<ReturnType<typeof setInterval> | undefined>(undefined);

  // Reflect the stored preference once mounted (no localStorage read during SSR).
  useEffect(() => setMuted(readMuted()), []);

  // Ramp the element volume toward `target` over `ms`, then optionally pause.
  const fadeTo = useCallback((target: number, ms: number, thenPause = false) => {
    const a = audioRef.current;
    if (!a) return;
    clearInterval(fadeRef.current);
    const steps = Math.max(1, Math.round(ms / 50));
    const from = a.volume;
    let i = 0;
    fadeRef.current = setInterval(() => {
      i += 1;
      a.volume = Math.min(1, Math.max(0, from + (target - from) * (i / steps)));
      if (i >= steps) {
        clearInterval(fadeRef.current);
        if (thenPause) a.pause();
      }
    }, 50);
  }, []);

  // (Re)create the audio element for the current theme.
  useEffect(() => {
    if (!themeUrl) return;
    const a = new Audio(themeUrl);
    a.loop = true;
    a.preload = 'auto';
    a.volume = 0;
    audioRef.current = a;

    const start = () => {
      if (readMuted()) return;
      const p = a.play();
      if (p && typeof p.then === 'function')
        p.then(() => fadeTo(TARGET_VOLUME, FADE_IN_MS)).catch(() => undefined);
      else fadeTo(TARGET_VOLUME, FADE_IN_MS);
    };

    // Autoplay-with-sound may still be blocked; unblock on the first gesture.
    const unblock = () => {
      if (a.paused) start();
    };
    document.addEventListener('pointerdown', unblock, { once: true });
    document.addEventListener('keydown', unblock, { once: true });

    start();

    return () => {
      document.removeEventListener('pointerdown', unblock);
      document.removeEventListener('keydown', unblock);
      // Stop any in-flight in-component fade (shared ref), then fade THIS element
      // out on its own interval so a remount's fade-in can't cancel it before it
      // pauses otherwise the old <audio loop> keeps playing forever.
      clearInterval(fadeRef.current);
      audioRef.current = null;
      fadeOutAndStop(a, FADE_OUT_MS);
    };
  }, [themeUrl, fadeTo]);

  const toggle = useCallback(() => {
    const next = !readMuted();
    try {
      localStorage.setItem(MUTE_KEY, next ? '1' : '0');
    } catch {
      /* private mode preference just won't persist */
    }
    setMuted(next);
    const a = audioRef.current;
    if (!a) return;
    if (next) {
      fadeTo(0, 250, true);
    } else {
      const p = a.play();
      if (p && typeof p.then === 'function') p.catch(() => undefined);
      fadeTo(TARGET_VOLUME, 400);
    }
  }, [fadeTo]);

  return { active: Boolean(themeUrl), muted, toggle };
}
