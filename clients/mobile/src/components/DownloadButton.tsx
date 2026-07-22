// Offline download affordance: idle arrow, live progress ring, done check.
// Long-press (or tap when done) removes the download.

import type { MediaItem } from '@kroma/core';
import { useMemo } from 'react';
import { Alert, Pressable, StyleSheet, View } from 'react-native';
import { useDownloads } from '../lib/downloads';
import { useT } from '../lib/i18n';
import { colors } from '../lib/theme';
import { CheckIcon, DownloadIcon } from '../player/icons';
import { ProgressRing } from './ProgressRing';

const RING = 34;

export function DownloadButton({ item, size = 22 }: Readonly<{ item: MediaItem; size?: number }>) {
  const t = useT();
  const downloads = useDownloads();
  const state = downloads.stateFor(item.id);
  const eligible = useMemo(() => downloads.canDownload(item), [downloads, item]);
  if (!eligible && state.status === 'none') return null;

  const onPress = () => {
    if (state.status === 'none') downloads.start(item);
    else if (state.status === 'queued') downloads.cancel(item.id);
    else if (state.status === 'downloading') {
      Alert.alert(t('offline.cancelDownload'), undefined, [
        { text: t('common.back'), style: 'cancel' },
        {
          text: t('offline.cancelDownload'),
          style: 'destructive',
          onPress: () => downloads.cancel(item.id),
        },
      ]);
    } else if (state.status === 'done') {
      Alert.alert(t('offline.downloaded'), undefined, [
        { text: t('common.cancel'), style: 'cancel' },
        {
          text: t('offline.remove'),
          style: 'destructive',
          onPress: () => void downloads.remove(item.id),
        },
      ]);
    }
  };

  let glyph = <DownloadIcon size={size} />;
  if (state.status === 'downloading') glyph = <ProgressRing progress={state.progress} />;
  else if (state.status === 'queued') glyph = <ProgressRing progress={-1} />;
  else if (state.status === 'done')
    glyph = (
      <View style={styles.doneBadge}>
        <CheckIcon size={size - 6} color={colors.accentInk} />
      </View>
    );

  return (
    <Pressable onPress={onPress} hitSlop={10} style={styles.box}>
      {glyph}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  box: { width: RING, height: RING, alignItems: 'center', justifyContent: 'center' },
  doneBadge: {
    width: RING - 6,
    height: RING - 6,
    borderRadius: (RING - 6) / 2,
    backgroundColor: colors.accent,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
