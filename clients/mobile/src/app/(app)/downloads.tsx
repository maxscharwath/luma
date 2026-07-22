// Offline downloads, Netflix-style: in-progress titles with a circular
// progress ring, finished titles as swipe-to-delete rows, storage footer and
// a clean empty state.

import { episodeTag, formatRuntime, type MediaItem } from '@kroma/core';
import { useRouter } from 'expo-router';
import { useRef } from 'react';
import { Alert, FlatList, Pressable, StyleSheet, Text, View } from 'react-native';
import ReanimatedSwipeable, {
  type SwipeableMethods,
} from 'react-native-gesture-handler/ReanimatedSwipeable';
import { FadeImage } from '../../components/FadeImage';
import { PageHeader } from '../../components/PageHeader';
import { ProgressRing } from '../../components/ProgressRing';
import { EmptyState, Screen } from '../../components/ui';
import { type DownloadEntry, formatBytes, useDownloads } from '../../lib/downloads';
import { useT } from '../../lib/i18n';
import { boxed, contentWidth } from '../../lib/layout';
import { useClient } from '../../lib/session';
import { colors, radius, spacing, type } from '../../lib/theme';
import { DownloadIcon, PlayIcon, TrashIcon } from '../../player/icons';

function RowArt({ uri, seed }: Readonly<{ uri: string | null; seed: string }>) {
  return (
    <View>
      <FadeImage uri={uri} seed={seed} radius={radius.sm} style={styles.thumb} />
      <View style={styles.playBadge}>
        <View style={styles.playCircle}>
          <PlayIcon size={15} />
        </View>
      </View>
    </View>
  );
}

function DownloadRow({ entry }: Readonly<{ entry: DownloadEntry }>) {
  const t = useT();
  const router = useRouter();
  const downloads = useDownloads();
  const swipeRef = useRef<SwipeableMethods>(null);
  const { item } = entry;
  const sub = [episodeTag(item), formatRuntime(item.durationMs), formatBytes(entry.sizeBytes)]
    .filter(Boolean)
    .join(' · ');

  return (
    <ReanimatedSwipeable
      ref={swipeRef}
      overshootRight={false}
      friction={1.6}
      rightThreshold={36}
      renderRightActions={() => (
        <Pressable
          onPress={() => {
            swipeRef.current?.close();
            void downloads.remove(item.id);
          }}
          style={styles.deleteAction}
        >
          <TrashIcon size={22} />
          <Text style={styles.deleteLabel}>{t('common.delete')}</Text>
        </Pressable>
      )}
    >
      <Pressable
        onPress={() => router.push(`/player/${item.id}` as never)}
        style={({ pressed }) => [styles.row, pressed && { backgroundColor: colors.surface }]}
      >
        <RowArt uri={entry.backdropUrl ?? entry.posterUrl} seed={item.id} />
        <View style={styles.text}>
          <Text numberOfLines={2} style={styles.rowTitle}>
            {item.showTitle ?? item.metadata?.title ?? item.title}
          </Text>
          <Text numberOfLines={1} style={styles.rowSub}>
            {sub}
          </Text>
        </View>
      </Pressable>
    </ReanimatedSwipeable>
  );
}

function activeLabel(t: ReturnType<typeof useT>, progress: number, queued?: boolean): string {
  if (queued) return t('offline.queued');
  if (progress >= 0) return t('offline.downloading', { percent: Math.round(progress * 100) });
  return t('offline.downloading', { percent: '' }).replace('%', '').trim();
}

function ActiveRow({
  item,
  progress,
  queued,
}: Readonly<{
  item: MediaItem;
  progress: number;
  queued?: boolean;
}>) {
  const t = useT();
  const client = useClient();
  const downloads = useDownloads();
  const confirmCancel = () =>
    Alert.alert(t('offline.cancelDownload'), undefined, [
      { text: t('common.back'), style: 'cancel' },
      {
        text: t('offline.cancelDownload'),
        style: 'destructive',
        onPress: () => downloads.cancel(item.id),
      },
    ]);
  return (
    <Pressable onPress={confirmCancel} style={styles.row}>
      <RowArt uri={client.backdropFor(item) ?? client.posterFor(item)} seed={item.id} />
      <View style={styles.text}>
        <Text numberOfLines={2} style={styles.rowTitle}>
          {item.showTitle ?? item.metadata?.title ?? item.title}
        </Text>
        <Text numberOfLines={1} style={styles.rowSub}>
          {activeLabel(t, progress, queued)}
        </Text>
      </View>
      <View style={styles.ringBox}>
        <ProgressRing progress={progress} size={36} />
      </View>
    </Pressable>
  );
}

export default function Downloads() {
  const t = useT();
  const downloads = useDownloads();
  const hasAnything =
    downloads.entries.length > 0 ||
    downloads.downloading.length > 0 ||
    downloads.queuedItems.length > 0;

  return (
    <Screen padded={false}>
      <PageHeader title={t('offline.downloads')} />
      {hasAnything ? (
        <FlatList
          data={downloads.entries}
          keyExtractor={(e) => e.itemId}
          renderItem={({ item }) => <DownloadRow entry={item} />}
          contentContainerStyle={styles.list}
          ListHeaderComponent={
            downloads.downloading.length > 0 || downloads.queuedItems.length > 0 ? (
              <View style={styles.activeBlock}>
                {downloads.downloading.map(({ item, progress }) => (
                  <ActiveRow key={item.id} item={item} progress={progress} />
                ))}
                {downloads.queuedItems.map((item) => (
                  <ActiveRow key={item.id} item={item} progress={-1} queued />
                ))}
              </View>
            ) : null
          }
          ListFooterComponent={
            downloads.entries.length > 0 ? (
              <Text style={styles.footer}>
                {t('offline.storageUsed', { size: formatBytes(downloads.totalBytes) })}
              </Text>
            ) : null
          }
        />
      ) : (
        <EmptyState
          icon={<DownloadIcon size={34} color={colors.textDim} />}
          title={t('offline.downloads')}
          hint={t('offline.empty')}
        />
      )}
    </Screen>
  );
}

const styles = StyleSheet.create({
  list: {
    paddingHorizontal: spacing.md,
    paddingBottom: spacing.xl,
    gap: 4,
    ...boxed(contentWidth.reading),
  },
  activeBlock: { gap: 4, marginBottom: spacing.sm },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    padding: 8,
    borderRadius: radius.md,
    backgroundColor: colors.bg,
  },
  thumb: { width: 130, height: 73 },
  playBadge: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    alignItems: 'center',
    justifyContent: 'center',
  },
  playCircle: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: 'rgba(10, 10, 12, 0.55)',
    alignItems: 'center',
    justifyContent: 'center',
  },
  text: { flex: 1, gap: 3 },
  rowTitle: { ...type.body, fontWeight: '600' },
  rowSub: { ...type.small },
  ringBox: { paddingRight: 4 },
  deleteAction: {
    width: 92,
    marginLeft: 8,
    borderRadius: radius.md,
    backgroundColor: colors.danger,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 4,
  },
  deleteLabel: { ...type.small, color: colors.text, fontWeight: '700' },
  footer: { ...type.small, textAlign: 'center', marginTop: spacing.md },
});
