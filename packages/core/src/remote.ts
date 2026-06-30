// Normalizes TV remote + keyboard input into logical keys, so the web, Tizen
// and webOS shells share one navigation model.

export type RemoteKey =
  | 'Up'
  | 'Down'
  | 'Left'
  | 'Right'
  | 'Enter'
  | 'Back'
  | 'Play'
  | 'Pause'
  | 'PlayPause'
  | 'Stop'
  | 'Rewind'
  | 'FastForward'
  | 'ColorRed'
  | 'ColorGreen'
  | 'ColorYellow'
  | 'ColorBlue';

// keyCode map covering desktop browsers, Tizen (Samsung) and webOS (LG) remotes.
const KEY_CODES: Record<number, RemoteKey> = {
  37: 'Left',
  38: 'Up',
  39: 'Right',
  40: 'Down',
  13: 'Enter',
  // Back: webOS=461, Tizen=10009, browser Backspace=8
  461: 'Back',
  10009: 'Back',
  8: 'Back',
  // Media transport (Tizen MediaPlay/Pause/etc.)
  415: 'Play',
  19: 'Pause',
  10252: 'PlayPause',
  413: 'Stop',
  412: 'Rewind',
  417: 'FastForward',
  // Colour buttons
  403: 'ColorRed',
  404: 'ColorGreen',
  405: 'ColorYellow',
  406: 'ColorBlue',
};

const KEY_NAMES: Record<string, RemoteKey> = {
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  Enter: 'Enter',
  Backspace: 'Back',
  Escape: 'Back',
  MediaPlay: 'Play',
  MediaPause: 'Pause',
  MediaPlayPause: 'PlayPause',
  MediaStop: 'Stop',
  MediaRewind: 'Rewind',
  MediaFastForward: 'FastForward',
};

/** Resolve a KeyboardEvent into a logical remote key, or null if unmapped. */
export function resolveRemoteKey(e: KeyboardEvent): RemoteKey | null {
  const named = e.key ? KEY_NAMES[e.key] : undefined;
  if (named) return named;
  return KEY_CODES[e.keyCode] ?? null;
}

/** A remote-key handler. Return `false` to mark the key *unhandled* (so the event
 * keeps its default e.g. let a text field own ◀ ▶ / Enter); anything else
 * (incl. `undefined`) counts as handled and the event is `preventDefault`-ed. */
export type RemoteKeyHandler = (e: KeyboardEvent) => void | boolean;
/** Declarative key → action table: `{ Enter: () => …, Back: () => … }`. */
export type RemoteKeyMap = Partial<Record<RemoteKey, RemoteKeyHandler>>;

/**
 * Resolve a keydown and dispatch it through a {@link RemoteKeyMap} so screens
 * declare *what* each key does instead of hand-rolling a `switch` with the same
 * resolve / auto-repeat / `preventDefault` plumbing every time. Returns the
 * resolved key (or null), so callers can fall through on unbound keys.
 *
 * `ignoreRepeat` lists keys whose auto-repeat is swallowed (discrete OK actions:
 * a held OK that entered a screen must not re-fire on the next one).
 */
export function dispatchRemoteKey(
  e: KeyboardEvent,
  map: RemoteKeyMap,
  opts: { ignoreRepeat?: readonly RemoteKey[] } = {},
): RemoteKey | null {
  const key = resolveRemoteKey(e);
  if (!key) return null;
  if (e.repeat && opts.ignoreRepeat?.includes(key)) {
    e.preventDefault();
    return key;
  }
  const handler = map[key];
  if (handler && handler(e) !== false) e.preventDefault();
  return key;
}

const TIZEN_KEYS = [
  'MediaPlay',
  'MediaPause',
  'MediaPlayPause',
  'MediaStop',
  'MediaRewind',
  'MediaFastForward',
  'ColorF0Red',
  'ColorF1Green',
  'ColorF2Yellow',
  'ColorF3Blue',
];

/**
 * On Tizen the media/colour keys must be explicitly registered before the
 * platform delivers them. No-op everywhere else. Call once at boot.
 */
export function registerTvMediaKeys(): void {
  const tizen = (globalThis as { tizen?: { tvinputdevice?: { registerKey(k: string): void } } })
    .tizen;
  const dev = tizen?.tvinputdevice;
  if (!dev) return;
  for (const k of TIZEN_KEYS) {
    try {
      dev.registerKey(k);
    } catch {
      /* key unavailable on this model ignore */
    }
  }
}
