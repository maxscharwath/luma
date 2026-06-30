import { useEffect, useState } from 'react';

/**
 * Focus state for the two "interrupt" overlays — the floating skip-intro button
 * and the up-next card (Play now / Cancel) — both independent of the auto-hiding
 * control bar. Each auto-focuses as its window opens; the player's key handler
 * (usePlayerControls) drives activation and hand-off.
 */
export function useOverlayFocus(canSkipIntro: boolean, upNext: boolean) {
  const [skipFocused, setSkipFocused] = useState(false);
  const [cardFocused, setCardFocused] = useState(false);
  const [upNextFocus, setUpNextFocus] = useState<0 | 1>(0); // 0 = play now, 1 = cancel

  // Auto-focus the skip button for the whole intro window.
  useEffect(() => {
    setSkipFocused(canSkipIntro);
  }, [canSkipIntro]);
  // Auto-focus the up-next card's "Play now" button when it appears.
  useEffect(() => {
    setCardFocused(upNext);
    if (upNext) setUpNextFocus(0);
  }, [upNext]);

  return {
    skipFocused,
    setSkipFocused,
    cardFocused,
    setCardFocused,
    upNextFocus,
    setUpNextFocus,
  } as const;
}
