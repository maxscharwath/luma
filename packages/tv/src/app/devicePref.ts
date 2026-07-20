// Device-scoped preferences, persisted in localStorage under the `kroma:*` keys.
//
// One place owns the storage rules every pref shares: reads and writes NEVER
// throw (a TV in a locked-down profile, private mode, or a storage quota can
// make localStorage unavailable at any moment), and an unknown stored value is
// treated as "unset" so a downgrade or a hand-edited key can't wedge a screen.
//
// Built on by enginePref, keyboardLayoutPref and the search history.

/** The raw stored value for a device key, or null when absent/unavailable. */
export function readDeviceValue(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null; /* storage unavailable */
  }
}

/** Persist a device key, best effort (a failed write is not worth an error). */
export function writeDeviceValue(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* storage unavailable */
  }
}

/** A persisted one-of-N device preference. */
export interface DevicePref<T extends string> {
  /** The stored choice, or `fallback` when unset / unknown / unreadable. */
  get(): T;
  /** Persist a choice (best effort). */
  set(value: T): void;
}

/** A device preference whose value is one of `values` (else `fallback`), stored
 * as-is under `key`. Callers wrap it in named get/set functions so each pref
 * keeps its own documented, typed surface. */
export function devicePref<T extends string>(
  key: string,
  values: readonly T[],
  fallback: T,
): DevicePref<T> {
  return {
    get() {
      const v = readDeviceValue(key);
      return v && (values as readonly string[]).includes(v) ? (v as T) : fallback;
    },
    set(value: T) {
      writeDeviceValue(key, value);
    },
  };
}
