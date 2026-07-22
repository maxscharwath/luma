// The player screen: full-screen expo-video surface + Kroma chrome. Locks
// landscape on phones, keeps the screen awake, resumes from saved progress,
// reports the playback heartbeat, and autoplays the next episode on end.

import { audioTracksOf, langCode, type MediaItem } from '@kroma/core';
import { useQuery } from '@tanstack/react-query';
import { useKeepAwake } from 'expo-keep-awake';
import { useLocalSearchParams, useNavigation, useRouter } from 'expo-router';
import type { VideoView as VideoViewRef } from 'expo-video';
import { VideoView } from 'expo-video';
import { useEffect, useRef, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { ErrorView, Loading } from '../../../components/ui';
import { type DownloadEntry, useDownloads } from '../../../lib/downloads';
import { useT } from '../../../lib/i18n';
import { useClient } from '../../../lib/session';
import { colors } from '../../../lib/theme';
import { useKromaEngine } from '../../../player/engine';
import { useHeartbeat } from '../../../player/heartbeat';
import { PlayerChrome } from '../../../player/PlayerChrome';
import { TrackSheet } from '../../../player/TrackSheet';
import { useStoryboard } from '../../../player/useStoryboard';
import { useSubtitles } from '../../../player/useSubtitles';

/** Resume from saved progress when meaningfully started and not basically done. */
function resumeSec(positionMs: number | undefined, durationMs: number | null): number {
  if (!positionMs || positionMs < 30_000) return 0;
  if (durationMs && positionMs > durationMs * 0.95) return 0;
  return positionMs / 1000;
}

function PlayerBody({
  item,
  startSec,
  localUri,
  offline,
}: Readonly<{
  item: MediaItem;
  startSec: number;
  localUri?: string;
  offline?: DownloadEntry;
}>) {
  const t = useT();
  const client = useClient();
  const router = useRouter();
  const engine = useKromaEngine(client, item, startSec, localUri);
  const navigation = useNavigation();
  const subs = useSubtitles(client, item, offline);
  const tileFor = useStoryboard(client, item, !localUri, offline);
  const [sheetOpen, setSheetOpen] = useState(false);
  const navigatedRef = useRef(false);
  const viewRef = useRef<VideoViewRef>(null);
  const next = useQuery({
    queryKey: ['next', item.id],
    queryFn: () => client.nextEpisode(item.id),
    enabled: !localUri && item.kind === 'episode',
    staleTime: 5 * 60_000,
  });

  useHeartbeat(client, item, () => ({
    positionSec: engine.cur,
    durationSec: engine.dur,
    playing: engine.playing,
    waiting: engine.waiting,
    mode: engine.mode,
    aac: engine.mode === 'master' && engine.filter !== 'off',
    audioLang: engine.offline
      ? engine.localAudio[engine.audioIndex]?.language || undefined
      : (audioTracksOf(item).find((a) => a.index === engine.audioIndex)?.language ?? undefined),
    subtitleLang:
      subs.active !== null
        ? langCode(subs.tracks.find((s) => s.index === subs.active)?.language)
        : undefined,
  }));

  // The screen leaving the stack for ANY reason (pop, replace, gesture) must
  // kill audio before the native dismissal even starts.
  useEffect(() => {
    return navigation.addListener('beforeRemove', () => engine.shutdown());
  }, [navigation, engine]);

  // Autoplay the next episode when playback naturally ends.
  useEffect(() => {
    if (engine.endedNonce === 0 || navigatedRef.current) return;
    navigatedRef.current = true;
    engine.shutdown();
    void client
      .nextEpisode(item.id)
      .then((next) => {
        if (next) router.replace(`/player/${next.id}` as never);
        else router.back();
      })
      .catch(() => router.back());
  }, [engine.endedNonce, engine, client, item.id, router]);

  if (engine.failed) {
    return (
      <ErrorView
        message={t('error.serverTitle')}
        retryLabel={t('player.back')}
        onRetry={() => router.back()}
      />
    );
  }

  return (
    <View style={styles.stage}>
      <VideoView
        ref={viewRef}
        player={engine.player}
        style={StyleSheet.absoluteFill}
        contentFit="contain"
        nativeControls={false}
        allowsPictureInPicture
        startsPictureInPictureAutomatically
      />
      <PlayerChrome
        engine={engine}
        item={item}
        cue={subs.cueAt(engine.cur)}
        onBack={() => {
          engine.shutdown();
          router.back();
        }}
        onOpenSheet={() => setSheetOpen(true)}
        onPip={() => viewRef.current?.startPictureInPicture()}
        tileFor={tileFor}
        next={next.data ?? null}
        onPlayNext={() => {
          navigatedRef.current = true;
          engine.shutdown();
          if (next.data) router.replace(`/player/${next.data.id}` as never);
        }}
      />
      <TrackSheet
        visible={sheetOpen}
        onClose={() => setSheetOpen(false)}
        engine={engine}
        subs={subs}
        item={item}
      />
    </View>
  );
}

export default function PlayerScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const t = useT();
  const client = useClient();
  useKeepAwake();

  const downloads = useDownloads();
  const dl = downloads.stateFor(id);
  const offline = dl.status === 'done' ? dl.entry : null;

  const item = useQuery({
    queryKey: ['item', id],
    queryFn: () => client.item(id),
    enabled: !offline,
  });
  const progress = useQuery({
    queryKey: ['progress', id],
    queryFn: () => client.itemProgress(id),
    staleTime: 0,
    retry: 0,
  });

  // A downloaded title plays from its on-device snapshot, network or not.
  if (offline) {
    return (
      <PlayerBody
        key={offline.itemId}
        item={offline.item}
        startSec={resumeSec(progress.data?.positionMs, offline.item.durationMs)}
        localUri={offline.fileUri}
        offline={offline}
      />
    );
  }

  if (item.isPending || progress.isPending) return <Loading label={t('common.loading')} />;
  if (item.isError)
    return (
      <ErrorView
        message={t('error.serverBody')}
        retryLabel={t('error.retry')}
        onRetry={() => item.refetch()}
      />
    );

  return (
    <PlayerBody
      key={item.data.id}
      item={item.data}
      startSec={resumeSec(progress.data?.positionMs, item.data.durationMs)}
    />
  );
}

const styles = StyleSheet.create({
  stage: { flex: 1, backgroundColor: colors.bg },
});
