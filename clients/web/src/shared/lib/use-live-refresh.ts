// Keep an open catalog fiche fresh without a manual reload. The server enriches
// metadata in the background (a scan, a nightly pass, or an operator correcting
// the TMDB match), then broadcasts `item.updated` / `show.updated` over the event
// stream. This hook listens for the ONE id the page is showing and invalidates
// its cache entry, so the new poster/title/synopsis swap in on their own.
//
// Invalidation is by key prefix, so `['show', id]` also refreshes the show's
// `['show', id, 'bundle']`. Bursts are coalesced (an enrich pass emits many
// updates for the same id) into a single refetch.

import { KromaEvents } from '@kroma/core';
import { useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { apiBase } from '#web/shared/lib/api';

export function useCatalogLiveRefresh(kind: 'item' | 'show', id: string): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    const want = kind === 'show' ? 'show.updated' : 'item.updated';
    let pending: ReturnType<typeof setTimeout> | null = null;
    const ev = new KromaEvents(apiBase(), {
      onEvent: (e) => {
        if (e.type !== want || e.id !== id || pending) return;
        pending = setTimeout(() => {
          pending = null;
          void queryClient.invalidateQueries({ queryKey: [kind, id] });
        }, 600);
      },
    });
    ev.connect();
    return () => {
      if (pending) clearTimeout(pending);
      ev.close();
    };
  }, [kind, id, queryClient]);
}
