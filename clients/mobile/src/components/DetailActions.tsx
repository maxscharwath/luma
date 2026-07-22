// Netflix-style action block for the detail pages: full-width accent play,
// full-width quiet download bar, then a centered row of equal-width icon-label
// actions (my list / watched / report) with an amber icon + label active state.

import type { MediaItem } from '@kroma/core';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { type DownloadState, useDownloads } from '../lib/downloads';
import { useT } from '../lib/i18n';
import { colors, radius, spacing, type } from '../lib/theme';
import {
  CheckIcon,
  DownloadIcon,
  EyeCheckIcon,
  EyeIcon,
  FlagIcon,
  PlayIcon,
  PlusIcon,
} from '../player/icons';

function downloadBarLabel(
  state: DownloadState,
  t: ReturnType<typeof useT>,
): { label: string; done: boolean } {
  if (state.status === 'done') return { label: t('offline.downloaded'), done: true };
  if (state.status === 'downloading')
    return {
      label:
        state.progress >= 0
          ? t('offline.downloading', { percent: Math.round(state.progress * 100) })
          : t('offline.downloading', { percent: '' }).replace('%', '').trim(),
      done: false,
    };
  return { label: t('offline.download'), done: false };
}

export function DetailActions({
  playLabel,
  onPlay,
  inList,
  onToggleList,
  watched,
  onToggleWatched,
  onReport,
  item,
}: Readonly<{
  playLabel: string;
  onPlay(): void;
  inList: boolean;
  onToggleList(): void;
  watched?: boolean;
  onToggleWatched?(): void;
  onReport?(): void;
  item: MediaItem;
}>) {
  const t = useT();
  const downloads = useDownloads();
  const state = downloads.stateFor(item.id);
  const bar = downloadBarLabel(state, t);
  return (
    <View style={styles.actions}>
      <Pressable
        onPress={onPlay}
        style={({ pressed }) => [styles.play, pressed && { opacity: 0.85 }]}
      >
        <PlayIcon size={22} color={colors.accentInk} />
        <Text style={styles.playLabel}>{playLabel}</Text>
      </Pressable>
      <Pressable
        onPress={() => {
          if (state.status === 'none') downloads.start(item);
          else if (state.status === 'downloading') downloads.cancel(item.id);
          else if (state.status === 'done') void downloads.remove(item.id);
        }}
        style={({ pressed }) => [styles.downloadBar, pressed && { opacity: 0.85 }]}
      >
        {state.status === 'downloading' && state.progress > 0 ? (
          <View
            style={[styles.downloadBarFill, { width: `${Math.round(state.progress * 100)}%` }]}
            pointerEvents="none"
          />
        ) : null}
        {bar.done ? (
          <CheckIcon size={20} color={colors.accent} />
        ) : (
          <DownloadIcon
            size={20}
            color={state.status === 'downloading' ? colors.accent : colors.text}
          />
        )}
        <Text style={[styles.downloadBarLabel, bar.done && { color: colors.accent }]}>
          {bar.label}
        </Text>
      </Pressable>
      <View style={styles.secondaryRow}>
        <Pressable
          onPress={onToggleList}
          style={({ pressed }) => [styles.secondary, pressed && { opacity: 0.7 }]}
        >
          {inList ? <CheckIcon size={24} color={colors.accent} /> : <PlusIcon size={24} />}
          <Text numberOfLines={1} style={[styles.secondaryLabel, inList && styles.secondaryActive]}>
            {t('nav.myList')}
          </Text>
        </Pressable>
        {onToggleWatched ? (
          <Pressable
            onPress={onToggleWatched}
            style={({ pressed }) => [styles.secondary, pressed && { opacity: 0.7 }]}
          >
            {watched ? <EyeCheckIcon size={24} color={colors.accent} /> : <EyeIcon size={24} />}
            <Text
              numberOfLines={1}
              style={[styles.secondaryLabel, watched && styles.secondaryActive]}
            >
              {t('content.watched')}
            </Text>
          </Pressable>
        ) : null}
        {onReport ? (
          <Pressable
            onPress={onReport}
            style={({ pressed }) => [styles.secondary, pressed && { opacity: 0.7 }]}
          >
            <FlagIcon size={24} />
            <Text numberOfLines={1} style={styles.secondaryLabel}>
              {t('reports.sheet')}
            </Text>
          </Pressable>
        ) : null}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  actions: { gap: spacing.sm },
  play: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 10,
    minHeight: 52,
    borderRadius: radius.md,
    backgroundColor: colors.accent,
  },
  playLabel: { color: colors.accentInk, fontSize: 16, fontWeight: '800' },
  downloadBar: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 10,
    minHeight: 48,
    borderRadius: radius.md,
    backgroundColor: colors.surfaceRaised,
    overflow: 'hidden',
  },
  downloadBarLabel: { color: colors.text, fontSize: 15, fontWeight: '700' },
  downloadBarFill: {
    position: 'absolute',
    left: 0,
    top: 0,
    bottom: 0,
    backgroundColor: colors.accentSoft,
    borderRadius: radius.md,
  },
  secondaryRow: {
    flexDirection: 'row',
    justifyContent: 'center',
    gap: spacing.lg,
    marginTop: spacing.xs,
  },
  secondary: { alignItems: 'center', gap: 5, width: 84, paddingVertical: 2 },
  secondaryLabel: { ...type.small, color: colors.textDim },
  secondaryActive: { color: colors.accent },
});
