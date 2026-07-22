// Netflix-style billboard: a tall rounded artwork card bleeding into the page
// background, with the title, a genre line and Play / My list actions.

import type { SectionItem } from '@kroma/core';
import { sizedImageUrl } from '@kroma/core';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { LinearGradient } from 'expo-linear-gradient';
import { useRouter } from 'expo-router';
import { Pressable, StyleSheet, Text, useWindowDimensions, View } from 'react-native';
import { useT } from '../lib/i18n';
import { useIsWide } from '../lib/layout';
import { useClient } from '../lib/session';
import { colors, radius, SHADE, spacing, type } from '../lib/theme';
import { CheckIcon, PlayIcon, PlusIcon } from '../player/icons';
import { FadeImage } from './FadeImage';

export function HeroBillboard({ entry }: Readonly<{ entry: SectionItem }>) {
  const t = useT();
  const client = useClient();
  const router = useRouter();
  const { width } = useWindowDimensions();
  const queryClient = useQueryClient();

  const media = entry.type === 'movie' ? entry.item : entry.show;
  const id = media.id;
  const title = media.metadata?.title ?? media.title;
  const genres = media.metadata?.genres?.slice(0, 3) ?? [];
  const detailRoute = entry.type === 'movie' ? `/item/${id}` : `/show/${id}`;
  // Narrow windows get the tall poster card; wide ones a backdrop billboard
  // whose height leaves the pill dock clear.
  const wide = useIsWide();
  const poster =
    entry.type === 'movie' ? client.posterFor(entry.item) : client.showPosterFor(entry.show);
  const art = wide
    ? (sizedImageUrl(client.backdropFor(media), 1600) ?? sizedImageUrl(poster, 1600))
    : (sizedImageUrl(poster, 780) ?? sizedImageUrl(client.backdropFor(media), 780));

  const myList = useQuery({ queryKey: ['myList'], queryFn: () => client.myList() });
  const inList = (myList.data ?? []).includes(id);
  const toggleList = useMutation({
    mutationFn: () => (inList ? client.removeFromList(id) : client.addToList(id)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ['myList'] }),
  });

  const play = () => {
    if (entry.type === 'movie') router.push(`/player/${entry.item.id}` as never);
    else router.push(detailRoute as never);
  };

  const w = Math.min(width - spacing.md * 2, wide ? 820 : 480);
  const h = wide ? Math.round(w * 0.52) : w * 1.42;

  return (
    <View style={[styles.wrap, { width: w, height: h }]}>
      <Pressable onPress={() => router.push(detailRoute as never)} style={StyleSheet.absoluteFill}>
        <FadeImage uri={art} seed={id} radius={radius.xl} style={StyleSheet.absoluteFill} />
        <LinearGradient
          colors={[SHADE.transparent, SHADE.transparent, SHADE.mid, SHADE.full]}
          locations={[0, 0.55, 0.78, 1]}
          style={[StyleSheet.absoluteFill, { borderRadius: radius.xl }]}
        />
      </Pressable>
      <View style={styles.content} pointerEvents="box-none">
        <Text numberOfLines={2} style={styles.title}>
          {title}
        </Text>
        {genres.length > 0 ? (
          <Text numberOfLines={1} style={styles.genres}>
            {genres.join('  ·  ')}
          </Text>
        ) : null}
        <View style={styles.buttons}>
          <Pressable
            onPress={play}
            style={({ pressed }) => [styles.play, pressed && { opacity: 0.85 }]}
          >
            <PlayIcon size={20} color={colors.accentInk} />
            <Text style={styles.playLabel}>{t('player.play')}</Text>
          </Pressable>
          <Pressable
            onPress={() => toggleList.mutate()}
            style={({ pressed }) => [styles.list, pressed && { opacity: 0.85 }]}
          >
            {inList ? <CheckIcon size={18} /> : <PlusIcon size={18} />}
            <Text style={styles.listLabel}>{t('nav.myList')}</Text>
          </Pressable>
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: { alignSelf: 'center', marginTop: spacing.sm },
  content: {
    position: 'absolute',
    left: spacing.md,
    right: spacing.md,
    bottom: spacing.md,
    alignItems: 'center',
    gap: 8,
  },
  title: {
    ...type.display,
    fontSize: 28,
    textAlign: 'center',
    textShadowColor: 'rgba(10, 10, 12, 0.85)',
    textShadowOffset: { width: 0, height: 1 },
    textShadowRadius: 10,
  },
  genres: { ...type.caption, color: colors.text },
  buttons: {
    flexDirection: 'row',
    gap: 10,
    marginTop: 6,
    alignSelf: 'center',
    width: '100%',
    maxWidth: 480,
  },
  play: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    minHeight: 46,
    borderRadius: radius.sm,
    backgroundColor: colors.accent,
  },
  playLabel: { color: colors.accentInk, fontSize: 15, fontWeight: '800' },
  list: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    minHeight: 46,
    borderRadius: radius.sm,
    backgroundColor: 'rgba(38, 38, 46, 0.85)',
  },
  listLabel: { color: colors.text, fontSize: 15, fontWeight: '600' },
});
