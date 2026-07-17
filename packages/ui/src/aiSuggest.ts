// Shared "Suggestions IA" poll loop behind each client's detail screen. The
// server generates the section lazily (LLM connector) and caches it, so the
// first calls return `null` (generating); we poll until a section arrives (its
// items may be empty, in which case the caller renders nothing) or we give up.
// The client is injected so web and TV share the exact same logic.

import type { KromaClient, Section } from '@kroma/core';
import { useEffect, useState } from 'react';

/** Re-poll cadence + ceiling while the model is still generating (×6s ≈ 72s). */
const POLL_MS = 6000;
const MAX_POLLS = 12;

export interface UseAiSuggestOptions {
  /** Gate polling (web waits for auth to hydrate). Default `true`. */
  active?: boolean;
}

export interface UseAiSuggestResult {
  /** The generated section once it arrives (items may be empty), else `null`. */
  section: Section | null;
  /** True while generating / waiting to start; false once terminal. */
  pending: boolean;
  /** Elapsed-time progress estimate (0..1) for a spinner while `pending`. */
  progress: number;
}

/**
 * Elapsed-time *estimate* (the LLM run is opaque): eases toward a 96% ceiling on
 * an exponential curve so it decelerates and never reads "done" until the work
 * actually arrives. τ=14s ⇒ ~90% at ~30s. Steps every 250ms; a CSS transition on
 * the ring interpolates between steps. Resets to 0 whenever it re-arms.
 */
function useEstimatedProgress(active: boolean): number {
  const [progress, setProgress] = useState(0);
  useEffect(() => {
    if (!active) return;
    setProgress(0);
    const start = Date.now();
    const id = setInterval(() => {
      setProgress(Math.min(0.96, 1 - Math.exp(-(Date.now() - start) / 14000)));
    }, 250);
    return () => clearInterval(id);
  }, [active]);
  return progress;
}

export function useAiSuggest(
  client: KromaClient,
  id: string,
  { active = true }: UseAiSuggestOptions = {},
): UseAiSuggestResult {
  const [section, setSection] = useState<Section | null>(null);
  const [pending, setPending] = useState(true);
  const progress = useEstimatedProgress(pending && active);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    let tries = 0;
    let timer: ReturnType<typeof setTimeout>;
    setSection(null);
    setPending(true);
    const poll = () => {
      client
        .aiSuggest(id)
        .then((res) => {
          if (cancelled) return;
          if (res) {
            // Terminal: a section (possibly with empty items).
            setSection(res);
            setPending(false);
          } else if (tries++ < MAX_POLLS) {
            timer = setTimeout(poll, POLL_MS); // still generating
          } else {
            setPending(false); // gave up waiting
          }
        })
        .catch(() => {
          if (!cancelled) setPending(false);
        });
    };
    poll();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [client, id, active]);

  return { section, pending, progress };
}
