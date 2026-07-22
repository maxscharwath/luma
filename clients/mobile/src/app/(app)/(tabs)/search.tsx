// Search: big title, rounded field, Netflix-style suggested list while the
// query is empty (personalized picks as landscape rows with a play affordance),
// debounced results as a poster grid.

import { type MediaItem, sizedImageUrl } from '@kroma/core';
import { useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import { FlatList, Pressable, StyleSheet, Text, useWindowDimensions, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { type CardModel, movieCard, showCard } from '../../../components/cards';
import { FadeImage } from '../../../components/FadeImage';
import { gridMetrics, PosterGrid } from '../../../components/PosterGrid';
import { SearchTabIcon } from '../../../components/tabIcons';
import { EmptyState, Loading, TextField } from '../../../components/ui';
import { useT } from '../../../lib/i18n';
import { useClient } from '../../../lib/session';
import { colors, radius, spacing, TAB_BAR_CLEARANCE, type } from '../../../lib/theme';
import { PlayIcon } from '../../../player/icons';

function useDebounced(value: string, ms: number): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return debounced;
}

function SuggestedRow({ item }: Readonly<{ item: MediaItem }>) {
  const client = useClient();
  const router = useRouter();
  return (
    <Pressable
      onPress={() => router.push(`/item/${item.id}` as never)}
      style={({ pressed }) => [styles.suggestRow, pressed && { backgroundColor: colors.surface }]}
    >
      <FadeImage
        uri={sizedImageUrl(client.backdropFor(item) ?? client.posterFor(item), 480)}
        seed={item.id}
        radius={radius.sm}
        style={styles.suggestThumb}
      />
      <Text numberOfLines={2} style={styles.suggestTitle}>
        {item.metadata?.title ?? item.title}
      </Text>
      <Pressable
        onPress={() => router.push(`/player/${item.id}` as never)}
        hitSlop={8}
        style={({ pressed }) => [
          styles.suggestPlay,
          pressed && { borderColor: colors.text, backgroundColor: colors.surfaceRaised },
        ]}
      >
        <PlayIcon size={16} />
      </Pressable>
    </Pressable>
  );
}

export default function Search() {
  const t = useT();
  const client = useClient();
  const { width } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const { cardW } = gridMetrics(width);
  const [query, setQuery] = useState('');
  const q = useDebounced(query.trim(), 300);

  const results = useQuery({
    queryKey: ['search', q],
    queryFn: () => client.search(q, { limit: 60 }),
    enabled: q.length >= 2,
    placeholderData: (prev) => prev,
  });
  const suggested = useQuery({
    queryKey: ['forYou'],
    queryFn: () => client.forYou(),
    staleTime: 10 * 60_000,
  });

  const cards: CardModel[] = (results.data?.results ?? []).map((hit) =>
    hit.type === 'show' ? showCard(hit.show, client, cardW) : movieCard(hit.item, client, cardW),
  );

  function searchBody(): React.ReactElement | null {
    if (q.length >= 2 && results.isPending) return <Loading label={t('common.loading')} />;
    if (q.length >= 2 && cards.length > 0) return <PosterGrid cards={cards} />;
    if (q.length >= 2 && results.isSuccess)
      return (
        <EmptyState
          icon={<SearchTabIcon color={colors.textDim} size={34} />}
          title={t('search.noResults')}
          hint={t('search.placeholder')}
        />
      );
    return (
      <FlatList
        data={(suggested.data ?? []).slice(0, 12)}
        keyExtractor={(item) => item.id}
        renderItem={({ item }) => <SuggestedRow item={item} />}
        ListHeaderComponent={<Text style={styles.suggestHeader}>{t('content.forYou')}</Text>}
        contentContainerStyle={{ paddingBottom: TAB_BAR_CLEARANCE }}
        keyboardShouldPersistTaps="handled"
      />
    );
  }

  return (
    <View style={[styles.screen, { paddingTop: insets.top + spacing.sm }]}>
      <View style={styles.inputBox}>
        <Text style={styles.pageTitle}>{t('nav.search')}</Text>
        <TextField
          value={query}
          onChangeText={setQuery}
          placeholder={t('search.placeholder')}
          returnKeyType="search"
          clearButtonMode="while-editing"
        />
      </View>
      {searchBody()}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.bg },
  inputBox: {
    paddingHorizontal: spacing.md,
    paddingBottom: spacing.md,
    gap: spacing.sm,
  },
  pageTitle: { ...type.display, fontSize: 30 },
  suggestHeader: { ...type.section, paddingHorizontal: spacing.md, marginBottom: spacing.sm },
  suggestRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingHorizontal: spacing.md,
    paddingVertical: 6,
  },
  suggestThumb: { width: 130, height: 73 },
  suggestTitle: { ...type.body, fontWeight: '600', flex: 1 },
  suggestPlay: {
    width: 38,
    height: 38,
    borderRadius: 19,
    borderWidth: 1.5,
    borderColor: colors.textDim,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
