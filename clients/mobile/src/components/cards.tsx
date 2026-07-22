// Poster + continue-watching cards and the horizontal media rail. Cards are
// pure presentation; navigation targets are resolved by the small helpers at
// the top so every screen routes titles the same way.

import {
  type ContinueItem,
  type KromaClient,
  type MediaItem,
  type SectionItem,
  type Show,
  sizedImageUrl,
} from '@kroma/core';
import { useRouter } from 'expo-router';
import { memo } from 'react';
import { FlatList, Pressable, StyleSheet, Text, useWindowDimensions, View } from 'react-native';
import { colors, posterWidth, radius, spacing, type } from '../lib/theme';
import { FadeImage } from './FadeImage';

export interface CardModel {
  key: string;
  title: string;
  subtitle?: string;
  poster: string | null;
  route: string;
}

export function movieCard(item: MediaItem, client: KromaClient, width: number): CardModel {
  return {
    key: item.id,
    title: item.metadata?.title ?? item.title,
    subtitle: item.year ? String(item.year) : undefined,
    poster: sizedImageUrl(client.posterFor(item), width * 2),
    route: `/item/${item.id}`,
  };
}

export function showCard(show: Show, client: KromaClient, width: number): CardModel {
  return {
    key: show.id,
    title: show.metadata?.title ?? show.title,
    subtitle: show.year ? String(show.year) : undefined,
    poster: sizedImageUrl(client.showPosterFor(show), width * 2),
    route: `/show/${show.id}`,
  };
}

export function sectionCard(entry: SectionItem, client: KromaClient, width: number): CardModel {
  return entry.type === 'movie'
    ? movieCard(entry.item, client, width)
    : showCard(entry.show, client, width);
}

export const PosterCard = memo(function PosterCard({
  card,
  width,
}: Readonly<{
  card: CardModel;
  width: number;
}>) {
  const router = useRouter();
  return (
    <Pressable
      onPress={() => router.push(card.route as never)}
      style={({ pressed }) => [{ width, opacity: pressed ? 0.75 : 1 }]}
    >
      <FadeImage
        uri={card.poster}
        seed={card.key}
        radius={radius.sm}
        style={{ width, height: width * 1.5 }}
      />
    </Pressable>
  );
});

export function MediaRail({ cards }: Readonly<{ cards: CardModel[] }>) {
  const { width: windowWidth } = useWindowDimensions();
  const width = posterWidth(windowWidth);
  return (
    <FlatList
      horizontal
      data={cards}
      keyExtractor={(c) => c.key}
      renderItem={({ item }) => <PosterCard card={item} width={width} />}
      showsHorizontalScrollIndicator={false}
      contentContainerStyle={styles.rail}
      initialNumToRender={6}
      windowSize={5}
      removeClippedSubviews
    />
  );
}

/** Landscape resume tile: backdrop, remaining-progress bar, episode tag. */
export function ContinueCard({
  entry,
  client,
  width,
}: Readonly<{
  entry: ContinueItem;
  client: KromaClient;
  width: number;
}>) {
  const router = useRouter();
  const { item, positionMs, durationMs } = entry;
  const total = durationMs ?? item.durationMs ?? 0;
  const frac = total > 0 ? Math.min(1, positionMs / total) : 0;
  const backdrop = sizedImageUrl(client.backdropFor(item) ?? client.posterFor(item), width * 2);
  const tag =
    item.season != null && item.episode != null ? `S${item.season}E${item.episode}` : undefined;
  return (
    <Pressable
      onPress={() => router.push(`/player/${item.id}` as never)}
      style={({ pressed }) => [{ width, opacity: pressed ? 0.75 : 1 }]}
    >
      <View>
        <FadeImage
          uri={backdrop}
          seed={item.id}
          radius={radius.sm}
          style={{ width, height: (width * 9) / 16 }}
        />
        <View style={styles.progressTrack}>
          <View style={[styles.progressFill, { width: `${frac * 100}%` }]} />
        </View>
      </View>
      <Text numberOfLines={1} style={styles.cardTitle}>
        {item.showTitle ?? item.metadata?.title ?? item.title}
      </Text>
      {tag ? <Text style={styles.cardSub}>{tag}</Text> : null}
    </Pressable>
  );
}

export function ContinueRail({
  entries,
  client,
}: Readonly<{
  entries: ContinueItem[];
  client: KromaClient;
}>) {
  const { width: windowWidth } = useWindowDimensions();
  const width = Math.min(300, windowWidth * 0.55);
  return (
    <FlatList
      horizontal
      data={entries}
      keyExtractor={(e) => e.item.id}
      renderItem={({ item }) => <ContinueCard entry={item} client={client} width={width} />}
      showsHorizontalScrollIndicator={false}
      contentContainerStyle={styles.rail}
    />
  );
}

const styles = StyleSheet.create({
  rail: { paddingHorizontal: spacing.md, gap: 12 },
  cardTitle: { ...type.caption, color: colors.text, marginTop: 6 },
  cardSub: { ...type.small, marginTop: 1 },
  progressTrack: {
    position: 'absolute',
    left: 6,
    right: 6,
    bottom: 6,
    height: 3,
    borderRadius: 2,
    backgroundColor: 'rgba(244, 243, 240, 0.3)',
  },
  progressFill: {
    height: 3,
    borderRadius: 2,
    backgroundColor: colors.accent,
  },
});
