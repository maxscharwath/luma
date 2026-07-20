// Reactive layer over devicePref: ONE shared, subscribable value per stored
// preference, so every mounted consumer (a menu row, the on-screen keyboard,
// the player) sees a change the moment any of them writes it. localStorage has
// no same-tab change events, so writes notify the in-process listeners here.

import { useSyncExternalStore } from 'react';
import { devicePref } from '#tv/app/devicePref';

/** A subscribable one-of-N device preference (devicePref + change notification). */
export interface ReactivePref<T extends string> {
  /** The current value (cached; storage is only read once at creation). */
  get(): T;
  /** Persist a new value and notify subscribers. Same-value writes are no-ops. */
  set(value: T): void;
  /** Listen for changes; returns the unsubscribe. */
  subscribe(listener: () => void): () => void;
}

/** A reactive one-of-N preference stored under `key` (unknown stored values
 * read as `fallback`, writes never throw - see devicePref). */
export function reactivePref<T extends string>(
  key: string,
  values: readonly T[],
  fallback: T,
): ReactivePref<T> {
  const stored = devicePref(key, values, fallback);
  const listeners = new Set<() => void>();
  let snapshot = stored.get();
  return {
    get: () => snapshot,
    set(value: T) {
      if (value === snapshot) return;
      snapshot = value;
      stored.set(value);
      for (const listener of listeners) listener();
    },
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

/** React binding: the component re-renders whenever the pref changes, from any
 * writer. Returns the `[value, set]` pair settings rows expect. */
export function useStoredPref<T extends string>(
  pref: ReactivePref<T>,
): readonly [T, (value: T) => void] {
  const value = useSyncExternalStore(pref.subscribe, pref.get, pref.get);
  return [value, pref.set] as const;
}
