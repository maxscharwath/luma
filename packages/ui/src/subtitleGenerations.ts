// Shared subtitle-generation poll loop, behind each client's player. Polls the
// in-flight generations for an item while `active`, fires `onComplete(subId)` once
// per generation that finishes (via a seen-set that persists across re-arms and
// re-mounts), and self-gates: the poll stops once nothing is in flight and re-arms
// on `active` toggles or a `refresh()` call (after the caller kicks off a new
// generation). The client is injected so web and TV share the exact same logic.

import type { KromaClient, SubtitleGeneration } from '@kroma/core';
import { useCallback, useEffect, useRef, useState } from 'react';

export interface SubtitleGenerationsOptions {
  /** Gate polling at all (web drives this off the drawer being open). Default `true`. */
  active?: boolean;
  /** Fired once per generation that reaches `done` with a resulting `subId`. */
  onComplete: (subId: string) => void;
}

export interface SubtitleGenerationsResult {
  /** Live (and recently-finished) generations, as last polled. */
  generations: SubtitleGeneration[];
  /** Optimistically drop a generation and request its cancellation server-side. */
  cancel: (genId: string) => void;
  /** Re-arm polling after the caller kicks off a new generation. */
  refresh: () => void;
}

export function useSubtitleGenerations(
  client: KromaClient,
  itemId: string,
  { active = true, onComplete }: SubtitleGenerationsOptions,
): SubtitleGenerationsResult {
  const [generations, setGenerations] = useState<SubtitleGeneration[]>([]);
  const [nudge, setNudge] = useState(0);
  const onCompleteRef = useRef(onComplete);
  onCompleteRef.current = onComplete;
  // Persist across re-arms/re-mounts so a finished generation only fires once
  // (reset when the item changes).
  const seenDoneRef = useRef<Set<string>>(new Set());
  const itemRef = useRef(itemId);
  if (itemRef.current !== itemId) {
    itemRef.current = itemId;
    seenDoneRef.current = new Set();
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: `nudge` is a trigger dep, not read in the body; bumping it via refresh() re-arms polling after a new generation is kicked off.
  useEffect(() => {
    if (!active) return;
    let stopped = false;
    let iv: ReturnType<typeof setInterval> | null = null;
    const stop = () => {
      if (iv) {
        clearInterval(iv);
        iv = null;
      }
    };
    const seenDone = seenDoneRef.current;
    const tick = async () => {
      try {
        const list = await client.subtitleGenerations(itemId);
        if (stopped) return;
        setGenerations(list);
        for (const g of list) {
          if (g.status === 'done' && g.subId && !seenDone.has(g.id)) {
            seenDone.add(g.id);
            onCompleteRef.current(g.subId);
          }
        }
        // Stop once there is nothing in flight (empty, or all terminal): a new
        // generation re-arms polling via `refresh`; toggling `active` re-runs.
        const live = list.some((g) => g.status !== 'done' && g.status !== 'error');
        if (!live) stop();
      } catch {
        /* transient; next tick retries */
      }
    };
    void tick();
    iv = setInterval(() => void tick(), 1500);
    return () => {
      stopped = true;
      stop();
    };
  }, [client, itemId, active, nudge]);

  const cancel = useCallback(
    (genId: string) => {
      setGenerations((prev) => prev.filter((g) => g.id !== genId));
      void client.cancelGeneration(itemId, genId).catch(() => undefined);
    },
    [client, itemId],
  );

  // Re-arm polling after the caller kicks off a new generation.
  const refresh = useCallback(() => setNudge((n) => n + 1), []);

  return { generations, cancel, refresh };
}
