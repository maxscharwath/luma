// Shared catalogue browser for the Films / Series tabs: large header with the
// title count, sort selector, genre filter chips, and an exact-fit poster grid.

import {
  collectGenres,
  hasGenre,
  type MediaItem,
  type Show,
  SORT_MODES,
  type SortMode,
  sortTitles,
} from '@kroma/core';
import { useMemo, useState } from 'react';
import { ScrollView, StyleSheet, Text, useWindowDimensions, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useT } from '../lib/i18n';
import { useClient } from '../lib/session';
import { colors, spacing, type } from '../lib/theme';
import { type CardModel, movieCard, showCard } from './cards';
import { gridMetrics, PosterGrid } from './PosterGrid';
import { FilmTabIcon } from './tabIcons';
import { Chip, EmptyState, ErrorView, Loading } from './ui';

const SORT_KEYS = {
  added: 'browse.sort.added',
  release: 'browse.sort.release',
  title: 'browse.sort.title',
  rating: 'browse.sort.rating',
} as const;

export function CatalogueScreen<T extends MediaItem | Show>({
  title,
  entries,
  kind,
  pending,
  error,
  refetch,
  refreshing,
}: Readonly<{
  title: string;
  entries: T[] | undefined;
  kind: 'movie' | 'show';
  pending: boolean;
  error: boolean;
  refetch(): void;
  refreshing: boolean;
}>) {
  const t = useT();
  const client = useClient();
  const { width } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const { cardW } = gridMetrics(width);
  const [sort, setSort] = useState<SortMode>('added');
  const [genre, setGenre] = useState<string | null>(null);

  const genres = useMemo(() => collectGenres(entries ?? []).slice(0, 14), [entries]);

  const cards: CardModel[] = useMemo(() => {
    const filtered = genre ? (entries ?? []).filter((e) => hasGenre(e, genre)) : (entries ?? []);
    const sorted = sortTitles(filtered, sort);
    return sorted.map((entry) =>
      kind === 'show'
        ? showCard(entry as Show, client, cardW)
        : movieCard(entry as MediaItem, client, cardW),
    );
  }, [entries, genre, sort, kind, client, cardW]);

  if (pending) return <Loading label={t('common.loading')} />;
  if (error)
    return (
      <ErrorView message={t('error.serverBody')} retryLabel={t('error.retry')} onRetry={refetch} />
    );

  const header = (
    <View style={{ paddingTop: insets.top + spacing.sm }}>
      <View style={styles.titleRow}>
        <Text style={styles.title}>{title}</Text>
        <Text style={styles.count}>{cards.length}</Text>
      </View>
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        contentContainerStyle={styles.chipRow}
        style={styles.chipStrip}
      >
        {SORT_MODES.map((mode) => (
          <Chip
            key={mode}
            label={t(SORT_KEYS[mode])}
            active={sort === mode}
            onPress={() => setSort(mode)}
          />
        ))}
      </ScrollView>
      {genres.length > 1 ? (
        <ScrollView
          horizontal
          showsHorizontalScrollIndicator={false}
          contentContainerStyle={styles.chipRow}
          style={styles.chipStrip}
        >
          <Chip
            label={t('browse.allGenres')}
            active={genre === null}
            onPress={() => setGenre(null)}
          />
          {genres.map((g) => (
            <Chip
              key={g.name}
              label={g.name}
              active={genre === g.name}
              onPress={() => setGenre(genre === g.name ? null : g.name)}
            />
          ))}
        </ScrollView>
      ) : null}
      <View style={{ height: spacing.sm }} />
    </View>
  );

  return (
    <View style={styles.screen}>
      <PosterGrid
        cards={cards}
        header={header}
        empty={
          <EmptyState
            icon={<FilmTabIcon color={colors.textDim} size={34} />}
            title={t('search.noResults')}
          />
        }
        refreshing={refreshing}
        onRefresh={refetch}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.bg },
  titleRow: { flexDirection: 'row', alignItems: 'baseline', gap: 10, marginBottom: spacing.sm },
  title: { ...type.display, fontSize: 30 },
  count: { ...type.caption },
  chipStrip: { marginHorizontal: -spacing.md, marginBottom: spacing.sm },
  chipRow: { gap: 8, paddingHorizontal: spacing.md },
});
