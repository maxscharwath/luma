// The contract every platform focus engine implements. Two implementations:
//
//   nav.ts      native  - Apple TV / Android TV. The OS focus engine owns
//                         directional movement; we only bridge Back / PlayPause
//                         and declare the preferred first responder.
//   nav.web.ts  web     - Tizen / webOS / desktop / browser. No OS focus engine,
//                         so we do geometric spatial navigation over the DOM
//                         nodes react-native-web renders for our focusables.
//
// Metro picks nav.ts, Vite picks nav.web.ts (resolve.extensions puts `.web.*`
// first). App code only ever imports from './nav'.

export interface FocusNavHandlers {
  /** Remote Back / Escape. Return `false` to say "not handled, keep the
   *  default"; returning nothing counts as handled. */
  // biome-ignore lint/suspicious/noConfusingVoidType: the union IS the contract here - a handler may return nothing (handled) or false (not handled).
  onBack?: () => void | boolean;
  /** Remote Play / Pause / PlayPause. */
  onPlayPause?: () => void;
  /** Re-run the mount behaviour when this changes (e.g. a view switch). */
  resetKey?: unknown;
}

/** Extra props the engine injects into the underlying Pressable of a
 * `<Focusable>`. Deliberately loose: each platform contributes a different set
 * (`hasTVPreferredFocus` natively, `dataSet` + `nativeID` on the web). */
export type FocusHostProps = Record<string, unknown>;

export interface FocusEngine {
  /** Wire the remote for the screen that mounts this. */
  useFocusNav(handlers: FocusNavHandlers): void;
  /** Props for one focusable host. */
  useFocusHostProps(opts: { autoFocus?: boolean; disabled?: boolean }): FocusHostProps;
}
