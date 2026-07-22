// react-native-tvos ships its TV surface as a `declare module 'react-native'`
// block INSIDE its own type package. TypeScript does not merge that: a package
// cannot augment itself, so the block is parsed as a shadowing ambient
// declaration and dropped (verified with both tsc and tsgo). We therefore
// restate the slice of the TV API this kit actually uses.
//
// `hasTVPreferredFocus` is NOT here: core React Native already declares it on
// ViewProps, so redeclaring it would conflict.

import 'react-native';

declare module 'react-native' {
  /** Hardware event from the TV remote. `eventType` is open-ended because each
   * platform adds its own ('menu', 'playPause', 'select', swipes, pan...). */
  export type HWEvent = {
    eventType: string;
    /** 0 = key down, 1 = key up. Absent on platforms that only report presses. */
    eventKeyAction?: number | undefined;
    tag?: number | undefined;
  };

  /** Subscribe to remote events the OS focus engine does not route to a
   * focusable (Back / Menu, transport keys, gestures). */
  export const useTVEventHandler: (handleEvent: (event: HWEvent) => void) => void;

  /** Claim TV-level keys and gestures from the system. */
  export const TVEventControl: {
    enableTVMenuKey(): void;
    disableTVMenuKey(): void;
    enableTVPanGesture(): void;
    disableTVPanGesture(): void;
    enableGestureHandlersCancelTouches(): void;
    disableGestureHandlersCancelTouches(): void;
  };

  interface ViewProps {
    /** Android TV: take part in the focus hierarchy. */
    tvFocusable?: boolean | undefined;
    /** Scroll snap alignment applied when this view takes focus inside a
     * ScrollView whose `snapToAlignment` is "item". */
    scrollSnapAlign?: 'start' | 'center' | 'end' | undefined;
  }
}
