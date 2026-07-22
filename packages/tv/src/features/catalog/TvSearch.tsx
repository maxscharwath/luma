import type { SearchHit } from '@kroma/core';
import { posterColors, qualityBadge, qualityBadgeForVideo } from '@kroma/core';
import { useT } from '@kroma/ui';
import { Box, Chip, Grid, PosterCard, TextField, Txt, useFocusNav } from '@kroma/ui/kit';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ScrollView } from 'react-native';
import { useConnection } from '#tv/app/providers/connection';
import { useEnv } from '#tv/app/providers/env';
import { useClient, useNav } from '#tv/app/router';
import { addRecentSearch, getRecentSearches } from '#tv/features/catalog/searchHistory';
import { KromaMark, OnScreenKeyboard, TvBackButton } from '#tv/shared/ui';

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
  const [recent, setRecent] = useState<string[]>(getRecentSearches);
  const { physicalKeyboard } = useEnv();
  useFocusNav({ onBack: nav.back });

  // A search "counts" once the user opens one of its results: remember the
  // query then, so the recent list holds real searches, not typing prefixes.
  const openHit = (h: Hit) => {
    setRecent(addRecentSearch(query));
    h.onOpen();
  };

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
    <Box fill z={10} bg="bg" px={64} py={44}>
      <Box row align="center" gap={14} mb={28}>
        <TvBackButton />
        <KromaMark size={28} />
        <Box flex />
        <Txt style={{ fontSize: 14, fontWeight: '600' }} color="textDim">
          {t('search.backHint')}
        </Txt>
      </Box>

      <Box row flex gap={52} style={{ minHeight: 0 }}>
        <Box w={520} shrink={0}>
          <TextField
            value={query}
            onChange={setQuery}
            icon="search"
            label={t('nav.search')}
            physicalKeyboard={physicalKeyboard}
            h={68}
            mb={26}
            bg="rgba(255, 255, 255, 0.05)"
            textStyle={{ fontSize: 24, fontWeight: '600' }}
          />
          <OnScreenKeyboard value={query} onChange={setQuery} onClose={nav.back} layout="search" />

          {/* recent searches: focusable pills that re-run the query */}
          {recent.length ? (
            <Box mt={28} gap={12} style={{ minHeight: 0 }}>
              <Txt style={RECENT_LABEL} color="textDim">
                {t('search.recent')}
              </Txt>
              <Box row wrap gap={10}>
                {recent.map((term) => (
                  <Chip
                    key={term}
                    variant="subtle"
                    focusScale={1.06}
                    label={term}
                    onPress={() => setQuery(term)}
                    style={{ maxWidth: 240, paddingHorizontal: 18, paddingVertical: 8 }}
                  />
                ))}
              </Box>
            </Box>
          ) : null}
        </Box>

        <ScrollView
          style={{ flex: 1, minHeight: 0 }}
          contentContainerStyle={{ paddingHorizontal: 20, paddingBottom: 32 }}
          showsVerticalScrollIndicator={false}
        >
          <Box row wrap align="center" gap={14} mb={18}>
            <Txt style={{ fontSize: 15, fontWeight: '700', letterSpacing: 0.6 }} color="textMuted">
              {t('search.results')}
            </Txt>
            <Txt style={{ fontSize: 12, fontWeight: '600' }} color="rgba(244, 243, 240, 0.34)">
              {t('search.hint')}
            </Txt>
          </Box>
          {/* The results pane is a fixed 1180px (1792 content - 520 keyboard -
              52 gap - 40 padding), so 4 columns of 277px with 24px gaps. */}
          {hits.length ? (
            <Grid width={RESULTS_WIDTH} columns={4} gap={24}>
              {hits.map((h) => (
                <PosterCard
                  key={h.id}
                  title={h.title}
                  art={h.poster}
                  tint={h.colors}
                  onPress={() => openHit(h)}
                />
              ))}
            </Grid>
          ) : (
            <Txt
              style={{ fontSize: 17, fontWeight: '500', paddingTop: 20 }}
              color="rgba(244, 243, 240, 0.4)"
            >
              {query.trim() ? t('search.noResults') : t('search.empty')}
            </Txt>
          )}
        </ScrollView>
      </Box>
    </Box>
  );
}

const RESULTS_WIDTH = 1180;
const RECENT_LABEL = { fontSize: 13, fontWeight: '700' as const, letterSpacing: 0.52 };
