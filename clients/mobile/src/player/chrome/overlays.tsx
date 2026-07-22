// Overlays that sit above the video independently of the controls: the subtitle
// cue line, the buffering spinner, skip-intro, and the up-next card.

import { type MediaItem, sizedImageUrl } from '@kroma/core';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';
import { FadeImage } from '../../components/FadeImage';
import { useT } from '../../lib/i18n';
import { useClient } from '../../lib/session';
import { absoluteFill, colors, radius } from '../../lib/theme';
import { PlayIcon } from '../icons';

/** The current subtitle line. It rides above the controls when they are up, so
 * the two never overlap. */
export function CueLine({ cue, bottom }: Readonly<{ cue: string; bottom: number }>) {
  if (!cue) return null;
  return (
    <View style={[styles.cueBox, { bottom }]}>
      <Text style={styles.cueText}>{cue}</Text>
    </View>
  );
}

export function BufferingSpinner() {
  return (
    <View style={styles.centerOverlay} pointerEvents="none">
      <ActivityIndicator size="large" color={colors.text} />
    </View>
  );
}

export function SkipIntroButton({
  onPress,
  bottom,
}: Readonly<{ onPress(): void; bottom: number }>) {
  const t = useT();
  return (
    <Pressable onPress={onPress} style={[styles.skipIntro, { bottom }]}>
      <Text style={styles.skipIntroText}>{t('player.skipIntro')}</Text>
    </Pressable>
  );
}

export function UpNextCard({
  next,
  onPlayNext,
  bottom,
}: Readonly<{
  next: MediaItem;
  onPlayNext(): void;
  bottom: number;
}>) {
  const t = useT();
  const client = useClient();
  const thumb = sizedImageUrl(client.backdropFor(next) ?? client.posterFor(next), 320);
  return (
    <Pressable onPress={onPlayNext} style={[styles.upNext, { bottom }]}>
      <FadeImage uri={thumb} seed={next.id} radius={6} style={styles.upNextThumb} />
      <View style={styles.upNextText}>
        <Text style={styles.upNextLabel}>{t('player.nextEpisode')}</Text>
        <Text numberOfLines={1} style={styles.upNextTitle}>
          {next.episodeTitle ?? next.title}
        </Text>
      </View>
      <PlayIcon size={20} />
    </Pressable>
  );
}

const styles = StyleSheet.create({
  centerOverlay: { ...absoluteFill, alignItems: 'center', justifyContent: 'center' },
  cueBox: { position: 'absolute', left: 40, right: 40, alignItems: 'center' },
  cueText: {
    color: colors.text,
    fontSize: 17,
    fontWeight: '600',
    textAlign: 'center',
    backgroundColor: 'rgba(10, 10, 12, 0.72)',
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 6,
    overflow: 'hidden',
  },
  skipIntro: {
    position: 'absolute',
    right: 32,
    backgroundColor: 'rgba(10, 10, 12, 0.8)',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: radius.sm,
    paddingHorizontal: 16,
    paddingVertical: 10,
  },
  skipIntroText: { color: colors.text, fontSize: 14, fontWeight: '600' },
  upNext: {
    position: 'absolute',
    right: 32,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    backgroundColor: 'rgba(10, 10, 12, 0.85)',
    borderWidth: 1,
    borderColor: colors.borderStrong,
    borderRadius: radius.md,
    padding: 8,
    paddingRight: 14,
    maxWidth: 320,
  },
  upNextThumb: { width: 84, height: 47 },
  upNextText: { flexShrink: 1 },
  upNextLabel: { color: colors.accent, fontSize: 11, fontWeight: '700' },
  upNextTitle: { color: colors.text, fontSize: 13, fontWeight: '600', marginTop: 2 },
});
