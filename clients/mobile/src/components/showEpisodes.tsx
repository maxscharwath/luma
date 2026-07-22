// Season-level building blocks for the series detail page: the bulk season
// download control, the up-next resume card and the episode rows.

import { formatRuntime, type MediaItem, type ProgressEntry, sizedImageUrl } from '@kroma/core';
import { useRouter } from 'expo-router';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { useDownloads } from '../lib/downloads';
import { useT } from '../lib/i18n';
import { useClient } from '../lib/session';
import { absoluteFill, colors, radius, type } from '../lib/theme';
import { CheckIcon, DownloadIcon, PlayIcon } from '../player/icons';
import { DownloadButton } from './DownloadButton';
import { FadeImage } from './FadeImage';
import { ProgressRing } from './ProgressRing';

/** Bulk season download: enqueues every not-yet-downloaded episode; while the
 * season is transferring it shows x/y and taps cancel the remainder. */
export function SeasonDownload({ episodes }: Readonly<{ episodes: MediaItem[] }>) {
  const t = useT();
  const downloads = useDownloads();
  const states = episodes.map((ep) => downloads.stateFor(ep.id));
  const done = states.filter((st) => st.status === 'done').length;
  const busy = states.filter((st) => st.status === 'downloading' || st.status === 'queued').length;

  if (episodes.length === 0) return null;
  if (done === episodes.length) {
    return (
      <View style={styles.seasonDl}>
        <CheckIcon size={16} color={colors.accent} />
        <Text style={[styles.seasonDlLabel, { color: colors.accent }]}>
          {t('offline.downloaded')}
        </Text>
      </View>
    );
  }
  if (busy > 0) {
    return (
      <Pressable
        onPress={() => {
          for (const ep of episodes) {
            const st = downloads.stateFor(ep.id);
            if (st.status === 'downloading' || st.status === 'queued') downloads.cancel(ep.id);
          }
        }}
        hitSlop={8}
        style={({ pressed }) => [styles.seasonDl, pressed && { opacity: 0.7 }]}
      >
        <ProgressRing progress={-1} size={18} />
        <Text style={styles.seasonDlLabel}>{`${done}/${episodes.length}`}</Text>
      </Pressable>
    );
  }
  return (
    <Pressable
      onPress={() => {
        for (const ep of episodes) downloads.start(ep);
      }}
      hitSlop={8}
      style={({ pressed }) => [styles.seasonDl, pressed && { opacity: 0.7 }]}
    >
      <DownloadIcon size={16} color={colors.text} />
      <Text style={styles.seasonDlLabel}>{t('offline.downloadSeason')}</Text>
    </Pressable>
  );
}

/** "Up next" resume card: backdrop thumb, progress sliver, episode title. */
export function UpNextCard({ next, frac }: Readonly<{ next: MediaItem; frac: number }>) {
  const t = useT();
  const client = useClient();
  const router = useRouter();
  return (
    <Pressable
      onPress={() => router.push(`/player/${next.id}` as never)}
      style={({ pressed }) => [styles.upNextCard, pressed && { opacity: 0.85 }]}
    >
      <View>
        <FadeImage
          uri={sizedImageUrl(client.backdropFor(next), 480)}
          seed={next.id}
          radius={radius.sm}
          style={styles.upNextThumb}
        />
        {frac > 0 ? (
          <View style={styles.upNextTrack}>
            <View style={[styles.upNextFill, { width: `${frac * 100}%` }]} />
          </View>
        ) : null}
      </View>
      <View style={styles.upNextText}>
        <Text style={styles.upNextLabel}>{t('content.upNext')}</Text>
        <Text numberOfLines={2} style={styles.upNextTitle}>
          {next.episode != null ? `${next.episode}. ` : ''}
          {next.episodeTitle ?? next.title}
        </Text>
      </View>
      <PlayIcon size={20} />
    </Pressable>
  );
}

export function EpisodeRow({
  episode,
  progress,
  watched,
}: Readonly<{
  episode: MediaItem;
  progress: ProgressEntry | undefined;
  watched: boolean;
}>) {
  const client = useClient();
  const router = useRouter();
  const runtime = formatRuntime(episode.durationMs);
  const total = progress?.durationMs ?? episode.durationMs ?? 0;
  const frac = progress && total > 0 ? Math.min(1, progress.positionMs / total) : 0;
  const overview = episode.metadata?.overview;
  return (
    <Pressable
      onPress={() => router.push(`/player/${episode.id}` as never)}
      onLongPress={() => router.push(`/item/${episode.id}` as never)}
      style={({ pressed }) => [styles.episode, pressed && { backgroundColor: colors.surface }]}
    >
      <View>
        <FadeImage
          uri={sizedImageUrl(client.backdropFor(episode), 480)}
          seed={episode.id}
          radius={radius.sm}
          style={styles.epThumb}
        />
        <View style={styles.epPlayBadge}>
          <View style={styles.epPlayCircle}>
            <PlayIcon size={15} />
          </View>
        </View>
        {frac > 0 ? (
          <View style={styles.epProgressTrack}>
            <View style={[styles.epProgressFill, { width: `${frac * 100}%` }]} />
          </View>
        ) : null}
      </View>
      <View style={styles.epText}>
        <View style={styles.epTitleRow}>
          <Text numberOfLines={1} style={styles.epTitle}>
            {episode.episode != null ? `${episode.episode}. ` : ''}
            {episode.episodeTitle ?? episode.title}
          </Text>
          {watched ? <CheckIcon size={14} color={colors.success} /> : null}
        </View>
        {runtime ? <Text style={styles.epMeta}>{runtime}</Text> : null}
        {overview ? (
          <Text numberOfLines={2} style={styles.epOverview}>
            {overview}
          </Text>
        ) : null}
      </View>
      <DownloadButton item={episode} />
    </Pressable>
  );
}

const styles = StyleSheet.create({
  seasonDl: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  seasonDlLabel: { ...type.caption, color: colors.text, fontWeight: '600' },
  upNextCard: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    backgroundColor: colors.surface,
    borderRadius: radius.md,
    padding: 8,
    paddingRight: 16,
  },
  upNextThumb: { width: 120, height: 68 },
  upNextTrack: {
    position: 'absolute',
    left: 5,
    right: 5,
    bottom: 5,
    height: 3,
    borderRadius: 2,
    backgroundColor: 'rgba(244, 243, 240, 0.3)',
  },
  upNextFill: { height: 3, borderRadius: 2, backgroundColor: colors.accent },
  upNextText: { flex: 1, gap: 2 },
  upNextLabel: { ...type.small, color: colors.accent, fontWeight: '700' },
  upNextTitle: { ...type.body, fontWeight: '600' },
  episode: {
    flexDirection: 'row',
    gap: 12,
    padding: 8,
    borderRadius: radius.md,
    alignItems: 'center',
  },
  epThumb: { width: 140, height: 79 },
  epPlayBadge: {
    ...absoluteFill,
    alignItems: 'center',
    justifyContent: 'center',
  },
  epPlayCircle: {
    width: 34,
    height: 34,
    borderRadius: 17,
    backgroundColor: 'rgba(10, 10, 12, 0.55)',
    alignItems: 'center',
    justifyContent: 'center',
  },
  epProgressTrack: {
    position: 'absolute',
    left: 5,
    right: 5,
    bottom: 5,
    height: 3,
    borderRadius: 2,
    backgroundColor: 'rgba(244, 243, 240, 0.3)',
  },
  epProgressFill: { height: 3, borderRadius: 2, backgroundColor: colors.accent },
  epText: { flex: 1, gap: 3 },
  epOverview: { ...type.small, color: colors.textDim, lineHeight: 17, fontWeight: '400' },
  epTitleRow: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  epTitle: { ...type.body, fontWeight: '600', flexShrink: 1 },
  epMeta: { ...type.small },
});
