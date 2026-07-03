import { useCallback, useEffect, useRef, useState } from 'react';

// One seek gesture model shared by the remote and the mouse. It turns a press
// (a held direction key, or a held rewind/forward button) into one of two
// behaviours and, crucially, tells them apart so a precise moment is easy to hit:
//
//   • TAP  - a short press jumps a fixed `TAP_STEP`. Quick successive taps STACK
//            (5 → 10 → 15 …) into one pending target and commit a SINGLE seek once
//            they settle, so tapping never rebuffers per-press and lands exactly.
//   • HOLD - a press held past `HOLD_MS` becomes a smooth, accelerating scrub
//            (slow at first so you can still stop on a dime, then fast to cross a
//            whole movie). Releasing commits the previewed position.
//
// A `scrub` path drives the same preview from an absolute position for mouse
// click / drag on the progress bar. Only ONE real seek is ever issued per gesture.

/** Seconds moved by one discrete tap; quick taps stack (5, 10, 15, …). */
const TAP_STEP = 5;
/** A press held longer than this (ms) turns into a continuous accelerating scrub. */
const HOLD_MS = 320;
/** Idle window (ms) after the last tap before the accumulated jump commits. */
const TAP_COMMIT_MS = 450;
/** Continuous-scrub speed model: media-seconds travelled per real second. */
const HOLD_BASE = 8;
/** Exponential growth of the hold speed, per second held. */
const HOLD_GROWTH = 3;
/** Speed ceiling so a long hold on a huge file still stays controllable. */
const HOLD_MAX = 600;

const nowMs = (): number =>
  typeof performance !== 'undefined' && performance.now ? performance.now() : Date.now();

export interface SeekDeps {
  /** Absolute current position (s). */
  getPosition: () => number;
  /** Total runtime (s), 0 when unknown. */
  duration: () => number;
  /** Commit a real, clamped seek to an absolute position (s). */
  seekTo: (absSec: number) => void;
}

export interface SeekGesture {
  /** Pending absolute target (s) during a tap-stack, hold-scrub or drag, else null. */
  preview: number | null;
  /** Begin a directional press (remote keydown / pointerdown on a seek button). */
  press: (dir: -1 | 1) => void;
  /** End the current directional press (keyup / pointerup / blur - wired globally). */
  release: () => void;
  /** A discrete directional tap (OK on a focused rewind/forward control). */
  tap: (dir: -1 | 1) => void;
  /** Live-preview an absolute position while clicking / dragging the scrub bar. */
  scrub: (absSec: number) => void;
  /** Commit the current preview (scrub-bar release / click). */
  commit: () => void;
}

export function useSeekGesture({ getPosition, duration, seekTo }: SeekDeps): SeekGesture {
  const [preview, setPreviewState] = useState<number | null>(null);
  const previewRef = useRef<number | null>(null);
  const setPreview = useCallback((v: number | null) => {
    previewRef.current = v;
    setPreviewState(v);
  }, []);

  const clamp = useCallback(
    (t: number) => {
      const total = duration();
      return Math.max(0, total > 0 ? Math.min(total - 1, t) : t);
    },
    [duration],
  );

  // The live gesture: direction, the accumulating target, and whether the press
  // has crossed into a continuous hold-scrub.
  const st = useRef({ dir: 1 as -1 | 1, target: 0, active: false, holding: false, holdAt: 0 });
  const holdTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const tapTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const raf = useRef<number | null>(null);

  const stopRaf = useCallback(() => {
    if (raf.current != null && typeof cancelAnimationFrame !== 'undefined') {
      cancelAnimationFrame(raf.current);
    }
    raf.current = null;
  }, []);

  const commit = useCallback(() => {
    if (tapTimer.current) {
      clearTimeout(tapTimer.current);
      tapTimer.current = null;
    }
    const target = previewRef.current;
    setPreview(null);
    if (target != null) seekTo(target);
  }, [seekTo, setPreview]);

  // Held past HOLD_MS: run an rAF loop that advances the target with a speed that
  // ramps up the longer the key is down. One preview per frame, one seek on release.
  const runHold = useCallback(() => {
    const s = st.current;
    s.holding = true;
    s.holdAt = nowMs();
    let last = s.holdAt;
    const tick = () => {
      if (!s.active) return;
      const t = nowMs();
      const dt = Math.min(0.1, (t - last) / 1000); // cap dt so a stall can't jump
      last = t;
      const elapsed = (t - s.holdAt) / 1000;
      const speed = Math.min(HOLD_MAX, HOLD_BASE * HOLD_GROWTH ** elapsed);
      s.target = clamp(s.target + s.dir * speed * dt);
      setPreview(s.target);
      raf.current = requestAnimationFrame(tick);
    };
    raf.current = requestAnimationFrame(tick);
  }, [clamp, setPreview]);

  const press = useCallback(
    (dir: -1 | 1) => {
      const s = st.current;
      if (s.active) return; // ignore key auto-repeat / a second button down
      if (tapTimer.current) {
        clearTimeout(tapTimer.current);
        tapTimer.current = null;
      }
      const base = previewRef.current ?? getPosition();
      s.dir = dir;
      s.active = true;
      s.holding = false;
      s.target = clamp(base + dir * TAP_STEP); // optimistic: show the first tap at once
      setPreview(s.target);
      holdTimer.current = setTimeout(runHold, HOLD_MS);
    },
    [clamp, getPosition, runHold, setPreview],
  );

  const release = useCallback(() => {
    const s = st.current;
    if (!s.active) return;
    s.active = false;
    if (holdTimer.current) {
      clearTimeout(holdTimer.current);
      holdTimer.current = null;
    }
    if (s.holding) {
      s.holding = false;
      stopRaf();
      commit(); // continuous scrub → seek immediately on release
    } else {
      // Was a tap: keep the accumulated preview and commit once taps settle, so a
      // burst of taps issues ONE seek and any next tap keeps stacking from here.
      tapTimer.current = setTimeout(commit, TAP_COMMIT_MS);
    }
  }, [commit, stopRaf]);

  const tap = useCallback(
    (dir: -1 | 1) => {
      press(dir);
      release();
    },
    [press, release],
  );

  const scrub = useCallback(
    (absSec: number) => {
      // A drag positions absolutely: cancel any directional press/tap in flight.
      const s = st.current;
      s.active = false;
      s.holding = false;
      if (holdTimer.current) {
        clearTimeout(holdTimer.current);
        holdTimer.current = null;
      }
      if (tapTimer.current) {
        clearTimeout(tapTimer.current);
        tapTimer.current = null;
      }
      stopRaf();
      setPreview(clamp(absSec));
    },
    [clamp, setPreview, stopRaf],
  );

  // Any key / pointer release (anywhere) ends a directional press - robust to the
  // pointer leaving the button, or a remote that only emits keyup at the very end.
  useEffect(() => {
    const onUp = () => release();
    window.addEventListener('keyup', onUp);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('blur', onUp);
    return () => {
      window.removeEventListener('keyup', onUp);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('blur', onUp);
    };
  }, [release]);

  // Flush a pending seek if the player unmounts mid-gesture.
  useEffect(
    () => () => {
      if (holdTimer.current) clearTimeout(holdTimer.current);
      if (tapTimer.current) clearTimeout(tapTimer.current);
      stopRaf();
      const target = previewRef.current;
      if (target != null) seekTo(target);
    },
    [seekTo, stopRaf],
  );

  return { preview, press, release, tap, scrub, commit };
}
