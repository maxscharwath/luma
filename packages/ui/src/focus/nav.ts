// Native focus engine (Apple TV / Android TV).
//
// The OS focus engine owns directional movement: UIFocusEngine on tvOS, the
// Android view hierarchy's nextFocus resolution on Android TV. That is a strict
// upgrade over the web's geometric scan (no getBoundingClientRect storm on every
// key press, real remote semantics, correct focus sounds), so this engine only
// bridges the two keys the OS does NOT route to a focusable: Back and PlayPause.
//
// OK is not handled here either: Pressable fires `onPress` on Select natively,
// and `<Focusable>` applies the press guard.

import { useEffect } from 'react';
import { BackHandler, type HWEvent, TVEventControl, useTVEventHandler } from 'react-native';
import { armPressGuard } from './guard';
import type { FocusHostProps, FocusNavHandlers } from './types';

/** tvOS delivers the remote's Menu button as this event once the menu key is
 * claimed; Android TV routes its Back button through BackHandler instead. */
const BACK_EVENTS = new Set(['menu', 'back']);
const PLAY_PAUSE_EVENTS = new Set(['playPause', 'play', 'pause']);

export function useFocusNav({ onBack, onPlayPause, resetKey }: FocusNavHandlers): void {
  // biome-ignore lint/correctness/useExhaustiveDependencies: resetKey is an intentional re-run trigger, mirroring the web engine; it is not read inside the effect.
  useEffect(() => {
    // Arm the guard on mount exactly like the web engine, so a held Select that
    // opened this screen cannot also fire the control the OS auto-focuses.
    armPressGuard();
  }, [resetKey]);

  useEffect(() => {
    if (!onBack) return;
    // Claim the Menu key so tvOS reports it instead of backing out of the app.
    TVEventControl.enableTVMenuKey();
    const sub = BackHandler.addEventListener('hardwareBackPress', () => onBack() !== false);
    return () => {
      sub.remove();
      TVEventControl.disableTVMenuKey();
    };
  }, [onBack]);

  useTVEventHandler((evt: HWEvent) => {
    // Key-up repeats carry eventKeyAction 1; only act on the press (0/undefined).
    if (evt.eventKeyAction === 1) return;
    if (BACK_EVENTS.has(evt.eventType)) onBack?.();
    else if (PLAY_PAUSE_EVENTS.has(evt.eventType)) onPlayPause?.();
  });
}

/** `hasTVPreferredFocus` is how a screen declares its entry point to the OS
 * focus engine; `focusable: false` removes a disabled control from it. */
export function useFocusHostProps({
  autoFocus,
  disabled,
}: {
  autoFocus?: boolean;
  disabled?: boolean;
}): FocusHostProps {
  if (disabled) return { focusable: false, tvFocusable: false };
  return { focusable: true, tvFocusable: true, hasTVPreferredFocus: autoFocus === true };
}
