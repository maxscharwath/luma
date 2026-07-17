// A typed, in-process event bus for loose coupling between modules. This is the
// frontend counterpart to the backend's open `ModuleEvent` envelope: modules
// declare their events by merging into `KromaEvents` (see contracts.ts), then
// emit/subscribe with full type-checking.

import type { KromaEvents } from './contracts';

export type EventKey = keyof KromaEvents & string;

export interface EventBus {
  emit<K extends EventKey>(key: K, payload: KromaEvents[K]): void;
  /** Subscribe; returns an unsubscribe function. */
  on<K extends EventKey>(key: K, handler: (payload: KromaEvents[K]) => void): () => void;
}

type AnyHandler = (payload: never) => void;

/** A minimal synchronous event bus. */
export function createEventBus(): EventBus {
  const handlers = new Map<string, Set<AnyHandler>>();
  return {
    emit(key, payload) {
      const set = handlers.get(key);
      if (!set) return;
      // Snapshot: a handler may (un)subscribe during dispatch; every handler
      // subscribed when emit began still fires exactly once.
      for (const handler of [...set]) (handler as (p: typeof payload) => void)(payload);
    },
    on(key, handler) {
      const set = handlers.get(key) ?? new Set<AnyHandler>();
      set.add(handler as AnyHandler);
      handlers.set(key, set);
      return () => {
        set.delete(handler as AnyHandler);
      };
    },
  };
}
