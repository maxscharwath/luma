import type { SearchHit } from '@luma/core';
import { posterColors, qualityBadge, qualityBadgeForVideo } from '@luma/core';
import { useT } from '@luma/ui';
import { IconSearch } from '@tabler/icons-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useClient, useNav } from '#tv/app/router';
import { useFocusNav } from '#tv/app/useFocusNav';
import { TvPoster } from '#tv/shared/TvMedia';
import { LumaMark, OnScreenKeyboard } from '#tv/shared/ui';

interface Hit {
  id: string;
  title: string;
  badge: string | null;
  poster: string;
  colors: [string, string];
  onOpen: () => void;
}

const DEBOUNCE_MS = 250;

/** Search with a D-pad on-screen keyboard (left) and a live results grid (right).
 * Queries the server's full-text engine (`/api/search` typo-tolerant, ranked
 * across title/cast/genre/overview), falling back to the in-memory catalogue when
 * the request fails. */
export function TvSearch() {
  const { movies, shows } = useConnection();
  const client = useClient();
  const t = useT();
  const nav = useNav();
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<Hit[]>([]);
  useFocusNav({ onBack: nav.back });

  const toHit = useCallback(
    (hit: SearchHit): Hit => {
      if (hit.type === 'show') {
        const s = hit.show;
        return {
          id: s.id,
          title: s.title,
          badge: qualityBadgeForVideo(s.video),
          poster: client.showPosterFor(s),
          colors: posterColors(s.id),
          onOpen: () => nav.go('show', { show: s }),
        };
      }
      const m = hit.item; // movie | episode both navigate to the item detail
      return {
        id: m.id,
        title: m.episodeTitle ?? m.title,
        badge: qualityBadge(m),
        poster: client.posterFor(m),
        colors: posterColors(m.id),
        onOpen: () => nav.go('movie', { item: m }),
      };
    },
    [client, nav],
  );

  // Offline fallback: filter the already-loaded catalogue by title / genre.
  const localHits = useCallback(
    (q: string): Hit[] => {
      const needle = q.toLowerCase();
      const match = (title: string, genres?: string[] | null) =>
        title.toLowerCase().includes(needle) ||
        (genres ?? []).some((g) => g.toLowerCase().includes(needle));
      const mv = movies
        .filter((m) => match(m.title, m.metadata?.genres))
        .map((m) => toHit({ type: 'movie', item: m }));
      const sh = shows
        .filter((s) => match(s.title, s.metadata?.genres))
        .map((s) => toHit({ type: 'show', show: s }));
      return [...mv, ...sh];
    },
    [movies, shows, toHit],
  );

  // Debounced server search; the latest query wins (stale responses are dropped).
  const seq = useRef(0);
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setHits([]);
      return;
    }
    const mine = ++seq.current;
    const timer = setTimeout(() => {
      client
        .search(q)
        .then((res) => {
          if (mine === seq.current) setHits(res.results.map(toHit));
        })
        .catch(() => {
          if (mine === seq.current) setHits(localHits(q)); // offline / server down
        });
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query, client, toHit, localHits]);

  return (
    <div className="fixed inset-0 z-10 flex flex-col bg-bg px-16 py-11 animate-[tv-fade-in_0.3s_ease]">
      <div className="mb-7 flex items-center gap-3.5">
        <LumaMark size={28} />
        <span className="ml-auto font-sans text-[14px] font-semibold text-dim">
          {t('search.backHint')}
        </span>
      </div>

      <div className="flex min-h-0 flex-1 gap-13">
        <div className="flex w-[520px] flex-none flex-col">
          <div className="mb-6.5 flex h-17 items-center gap-3.5 rounded-2xl border border-border-strong bg-[rgba(255,255,255,0.05)] px-5.5">
            <IconSearch size={24} stroke={1.8} color="rgba(244,243,240,0.5)" />
            <span className="flex-1 truncate font-sans text-[24px] font-semibold text-text">
              {query}
            </span>
            <span className="h-7 w-0.5 bg-accent animate-[tv-breathe_1.1s_ease-in-out_infinite]" />
          </div>
          <OnScreenKeyboard value={query} onChange={setQuery} onClose={nav.back} layout="search" />
        </div>

        <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto px-5 pb-8">
          <div className="mb-4.5 flex flex-wrap items-center gap-3.5">
            <span className="font-sans text-[15px] font-bold tracking-[0.04em] text-muted">
              {t('search.results')}
            </span>
            <span className="font-sans text-[12px] font-semibold text-[rgba(244,243,240,0.34)]">
              {t('search.hint')}
            </span>
          </div>
          {hits.length ? (
            <div className="grid grid-cols-4 gap-6">
              {hits.map((h) => (
                <TvPoster
                  key={h.id}
                  title={h.title}
                  poster={h.poster}
                  colors={h.colors}
                  onClick={h.onOpen}
                />
              ))}
            </div>
          ) : (
            <p className="pt-5 font-sans text-[17px] font-medium text-[rgba(244,243,240,0.4)]">
              {query.trim() ? t('search.noResults') : t('search.empty')}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
