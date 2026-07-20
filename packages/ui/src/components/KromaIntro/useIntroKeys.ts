import { useEffect, useRef } from 'react';

export interface IntroKeysParams {
  /** Skip to the app: OK/Enter, Space, Back/Escape. */
  exit: () => void;
  /** Restart the run from frame 0 (the `r` key, for tuning the choreography). */
  replay: () => void;
  /** First pointer/key of the session. Browsers block autoplay-with-sound until
   * a user gesture, so this is where each intro un-blocks its own audio. */
  unblock: () => void;
}

/**
 * The intro's input layer: the skip / replay keys plus the autoplay-unblock
 * gesture, registered once for the life of the intro.
 *
 * Skip keys are taken on the capture phase at `window` so the TV's spatial
 * focus-nav underneath stays inert, and `stopImmediatePropagation` means a skip
 * key never also reaches `unblock` (skipping must not restart anything).
 * One stable pair of listeners calls the latest closures.
 */
export function useIntroKeys({ exit, replay, unblock }: IntroKeysParams): void {
  const latest = useRef<IntroKeysParams>({ exit, replay, unblock });
  latest.current = { exit, replay, unblock };

  useEffect(() => {
    const onGesture = () => latest.current.unblock();
    document.addEventListener('pointerdown', onGesture);
    document.addEventListener('keydown', onGesture);

    // Skip / replay via keyboard + TV remote (OK/Enter, Space, Back/Escape).
    const onKey = (e: KeyboardEvent) => {
      const k = e.key;
      if (
        k === 'Enter' ||
        k === ' ' ||
        k === 'Spacebar' ||
        k === 'Escape' ||
        k === 'GoBack' ||
        k === 'BrowserBack'
      ) {
        e.preventDefault();
        e.stopImmediatePropagation();
        latest.current.exit();
      } else if (k === 'r' || k === 'R') {
        e.preventDefault();
        e.stopImmediatePropagation();
        latest.current.replay();
      }
    };
    // Capture phase so the TV's spatial focus-nav underneath stays inert.
    window.addEventListener('keydown', onKey, true);

    return () => {
      document.removeEventListener('pointerdown', onGesture);
      document.removeEventListener('keydown', onGesture);
      window.removeEventListener('keydown', onKey, true);
    };
  }, []);
}
