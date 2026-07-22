// Shared building blocks for the movie / series detail pages: the cinematic
// hero (backdrop, layered gradient, big title, meta badges) and the circular
// cast rail. The Netflix-style action block lives in DetailActions.tsx.

import type { CastMember } from '@kroma/core';
import { sizedImageUrl } from '@kroma/core';
import { LinearGradient } from 'expo-linear-gradient';
import { useRouter } from 'expo-router';
import type { ReactNode } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, useWindowDimensions, View } from 'react-native';
import Animated, { type SharedValue, useAnimatedStyle } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useClient } from '../lib/session';
import { colors, SHADE, spacing, type } from '../lib/theme';
import { BackIcon } from '../player/icons';
import { Avatar } from './Avatar';
import { FadeImage } from './FadeImage';

export { DetailActions } from './DetailActions';

export function MetaBadge({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <View style={styles.badge}>
      <Text style={styles.badgeText}>{children}</Text>
    </View>
  );
}

/** Backdrop hero with a 5-stop shade into the page, back button and a bottom
 * overlay carrying the display title, an optional context line and meta row.
 * When `scrollY` is provided the artwork stretches elastically on pull-down. */
export function DetailHero({
  art,
  seed,
  title,
  context,
  meta,
  scrollY,
}: Readonly<{
  art: string | null;
  seed: string;
  title: string;
  context?: string;
  meta?: ReactNode;
  scrollY?: SharedValue<number>;
}>) {
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const height = Math.min(width * 0.62, 460);

  const stretch = useAnimatedStyle(() => {
    // Only overscroll (y < 0) stretches: scale from the top anchor with the
    // gap halved away, the classic elastic-header recipe.
    const y = Math.min(scrollY?.value ?? 0, 0);
    return {
      transform: [{ translateY: y / 2 }, { scale: 1 - y / height }],
    };
  });

  return (
    <View style={{ height }}>
      <Animated.View style={[StyleSheet.absoluteFill, stretch]}>
        <FadeImage uri={art} seed={seed} style={StyleSheet.absoluteFill} />
        <LinearGradient
          colors={[SHADE.mid, SHADE.transparent, SHADE.transparent, SHADE.mid, SHADE.full]}
          locations={[0, 0.2, 0.45, 0.75, 1]}
          style={StyleSheet.absoluteFill}
        />
      </Animated.View>
      <Pressable
        onPress={() => router.back()}
        hitSlop={12}
        style={[styles.back, { top: insets.top + 6 }]}
      >
        <BackIcon />
      </Pressable>
      <View style={styles.heroText}>
        {context ? <Text style={styles.context}>{context}</Text> : null}
        <Text numberOfLines={2} style={styles.heroTitle}>
          {title}
        </Text>
        {meta ? <View style={styles.metaRow}>{meta}</View> : null}
      </View>
    </View>
  );
}

/** Circular cast photos with an initials fallback when no photo exists;
 * tapping a member opens their credits page. */
export function CastRail({ cast }: Readonly<{ cast: CastMember[] }>) {
  const client = useClient();
  const router = useRouter();
  if (cast.length === 0) return null;
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      contentContainerStyle={styles.castRail}
    >
      {cast.slice(0, 15).map((member) => (
        <Pressable
          key={member.name}
          onPress={() => router.push(`/person/${encodeURIComponent(member.name)}` as never)}
          style={({ pressed }) => [styles.castCard, pressed && { opacity: 0.7 }]}
        >
          <Avatar
            uri={sizedImageUrl(client.resolveArt(member.profileUrl), 320)}
            name={member.name}
            size={84}
          />
          <Text numberOfLines={2} style={styles.castName}>
            {member.name}
          </Text>
          {member.character ? (
            <Text numberOfLines={1} style={styles.castRole}>
              {member.character}
            </Text>
          ) : null}
        </Pressable>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  back: {
    position: 'absolute',
    left: spacing.md,
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: 'rgba(10, 10, 12, 0.55)',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 2,
  },
  heroText: {
    position: 'absolute',
    left: spacing.md,
    right: spacing.md,
    bottom: spacing.sm,
    gap: 6,
  },
  context: { ...type.caption, color: colors.accent, fontWeight: '700' },
  heroTitle: {
    ...type.display,
    fontSize: 32,
    textShadowColor: 'rgba(10, 10, 12, 0.85)',
    textShadowOffset: { width: 0, height: 1 },
    textShadowRadius: 10,
  },
  metaRow: { flexDirection: 'row', alignItems: 'center', gap: 8, flexWrap: 'wrap' },
  badge: {
    backgroundColor: 'rgba(28, 28, 34, 0.85)',
    borderRadius: 7,
    paddingHorizontal: 8,
    paddingVertical: 3,
  },
  badgeText: { fontSize: 12, fontWeight: '600', color: colors.text },
  castRail: { paddingHorizontal: spacing.md, gap: 12 },
  castCard: { width: 92, alignItems: 'center' },
  castName: {
    ...type.small,
    color: colors.text,
    marginTop: 6,
    textAlign: 'center',
    lineHeight: 14,
  },
  castRole: { ...type.small, fontSize: 10, marginTop: 1, textAlign: 'center' },
});
