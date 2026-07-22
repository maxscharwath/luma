// Web focus engine (Tizen / webOS / desktop / browser).
//
// No OS focus engine here, so directional movement is geometric (spatial.web.ts)
// over the DOM nodes react-native-web renders for our `<Focusable>`s, which tag
// themselves with `data-focus`.
//
// OK is deliberately NOT dispatched here. A `<Focusable>` renders a real
// <button>, so the browser's own activation behaviour turns Enter into a click,
// which react-native-web turns into `onPress`. All this engine does for OK is
// swallow the AUTO-REPEATS, so a held remote button cannot fire the control
// dozens of times (the single-press guard lives in `<Focusable>`).

import { dispatchRemoteKey, registerTvMediaKeys } from '@kroma/core';
import { useEffect } from 'react';
import { armPressGuard } from './guard';
import { focusFirst, inTextField, moveFocus } from './spatial.web';
import type { FocusHostProps, FocusNavHandlers } from './types';

export function useFocusNav({ onBack, onPlayPause, resetKey }: FocusNavHandlers): void {
  // biome-ignore lint/correctness/useExhaustiveDependencies: resetKey is an intentional re-run trigger (a view switch re-focuses the first element); it is not read inside the effect.
  useEffect(() => {
    registerTvMediaKeys();
    // Arm the guard before the listener attaches, so the press that navigated
    // here cannot beat it and activate the control we auto-focus below.
    armPressGuard();
    focusFirst();

    const onKey = (e: KeyboardEvent) => {
      // When a text field is focused, let it own the horizontal keys (cursor)
      // and Backspace (edit); only the vertical keys leave the field. Otherwise
      // typing a server URL is impossible.
      const inText = inTextField();
      // Media keys keep their native default (no preventDefault): handlers that
      // return `false` are treated as "not handled" by dispatchRemoteKey.
      const media = () => {
        onPlayPause?.();
        return false as const;
      };
      dispatchRemoteKey(
        e,
        {
          Back: (ev) => {
            // Already consumed by the on-screen keyboard's typing bridge (which
            // preventDefaults the Backspace it turned into a delete). Both
            // listeners sit on window, so without this one press would delete a
            // character AND leave the screen.
            if (ev.defaultPrevented) return false;
            // In a real text field a physical Backspace edits the value (native);
            // only Escape / a remote Back button leaves the screen.
            if (inText && ev.key === 'Backspace') return false;
            return onBack?.();
          },
          Play: media,
          Pause: media,
          PlayPause: media,
          Up: () => moveFocus('Up'),
          Down: () => moveFocus('Down'),
          Left: () => (inText ? false : moveFocus('Left')),
          Right: () => (inText ? false : moveFocus('Right')),
          // Not handled: the browser activates the focused <button> itself.
          Enter: () => false as const,
        },
        // A held OK auto-repeats; `ignoreRepeat` preventDefaults those, which
        // stops the browser from re-activating the button on every repeat.
        { ignoreRepeat: ['Enter'] },
      );
    };

    window.addEventListener('keydown', onKey);

    // No hover-focus: the amber ring moves on D-pad / arrow keys only (a mouse
    // still clicks natively, and clicking focuses the control). Cursor-follow
    // focus was tried and dropped on request: it fought physical typing and made
    // the ring jitter across the on-screen keyboard.
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, [onBack, onPlayPause, resetKey]);
}

/** `data-focus` is what spatial.web.ts queries for; `data-autofocus` marks the
 * screen's entry point (the web equivalent of tvOS `hasTVPreferredFocus`). A
 * disabled focusable is skipped by the geometry AND by the tab order. */
export function useFocusHostProps({
  autoFocus,
  disabled,
}: {
  autoFocus?: boolean;
  disabled?: boolean;
}): FocusHostProps {
  if (disabled) return { tabIndex: -1 };
  return { dataSet: autoFocus ? { focus: '', autofocus: '' } : { focus: '' } };
}
