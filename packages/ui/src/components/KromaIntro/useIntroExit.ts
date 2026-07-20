import { type RefObject, useCallback, useRef, useState } from 'react';
import { EXIT_MS } from './constants';

/** The exit/timer bundle both intros (video film + CSS fallback) run on. */
export interface IntroExit {
  /** True once the fade-to-black started; drives the exit veil. */
  exiting: boolean;
  /** Latched by {@link IntroExit.exit}. Late media events check it so a run the
   * user already skipped can neither restart nor cancel its hand-off. */
  exitedRef: RefObject<boolean>;
  /** The stall-safety timer. The intro arms it (its length depends on the
   * medium); this hook only clears it on exit / replay / unmount. */
  safetyRef: RefObject<ReturnType<typeof setTimeout> | undefined>;
  /** Fade to black, then hand off to `onDone`. Runs at most once per run. */
  exit: () => void;
  /** Re-open the timeline for a replay: cancel the pending hand-off + fade. */
  reopen: () => void;
  /** Drop both timers (unmount cleanup). */
  clearTimers: () => void;
}

/**
 * Shared end-of-intro lifecycle: a single-shot `exit()` that fades to black and
 * hands off to the app one {@link EXIT_MS} later, plus the two timers around it.
 *
 * The hand-off is a timer rather than a transition callback so it fires even on
 * TV webviews that drop `transitionend`, and `exitedRef` makes it idempotent:
 * whatever ends the intro first (film `ended`, a skip key, the stall safety)
 * wins, and `onDone` is called exactly once.
 *
 * `onDone` is read through a ref so the caller's mount-only effects never have
 * to re-run (which would restart the intro) when the prop identity changes.
 */
export function useIntroExit(onDone: () => void): IntroExit {
  const [exiting, setExiting] = useState(false);
  const safetyRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const exitRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const exitedRef = useRef(false);
  const onDoneRef = useRef(onDone);
  onDoneRef.current = onDone;

  const exit = useCallback(() => {
    if (exitedRef.current) return;
    exitedRef.current = true;
    clearTimeout(safetyRef.current);
    setExiting(true);
    exitRef.current = setTimeout(() => onDoneRef.current(), EXIT_MS);
  }, []);

  const reopen = useCallback(() => {
    exitedRef.current = false;
    clearTimeout(safetyRef.current);
    clearTimeout(exitRef.current);
    setExiting(false);
  }, []);

  const clearTimers = useCallback(() => {
    clearTimeout(safetyRef.current);
    clearTimeout(exitRef.current);
  }, []);

  return { exiting, exitedRef, safetyRef, exit, reopen, clearTimers };
}
