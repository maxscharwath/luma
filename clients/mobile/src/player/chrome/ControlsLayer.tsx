// The controls themselves: scrims, title bar, transport row and the scrub bar
// with its actions. Mounted only while the controls are up; every press pokes
// the auto-hide timer so touching a control keeps them on screen.

import { episodeTag, formatTimecode, type MediaItem } from '@kroma/core';
import { LinearGradient } from 'expo-linear-gradient';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import type { EdgeInsets } from 'react-native-safe-area-context';
import { useT } from '../../lib/i18n';
import { absoluteFill, colors, spacing } from '../../lib/theme';
import type { Engine } from '../engine';
import {
  Back10Icon,
  BackIcon,
  Forward10Icon,
  PauseIcon,
  PipIcon,
  PlayIcon,
  TracksIcon,
} from '../icons';
import { ScrubBar } from '../ScrubBar';
import type { StoryboardTile } from '../useStoryboard';

export function ControlsLayer({
  engine,
  item,
  insets,
  poke,
  onBack,
  onOpenSheet,
  tileFor,
  next,
  onPlayNext,
  onPip,
}: Readonly<{
  engine: Engine;
  item: MediaItem;
  insets: EdgeInsets;
  /** Restart the auto-hide countdown. */
  poke(): void;
  onBack(): void;
  onOpenSheet(): void;
  tileFor?: (abs: number) => StoryboardTile | null;
  next?: MediaItem | null;
  onPlayNext?(): void;
  onPip?(): void;
}>) {
  const t = useT();
  const title = item.showTitle ?? item.metadata?.title ?? item.title;
  const sub = episodeTag(item) ?? undefined;

  return (
    <View style={StyleSheet.absoluteFill} pointerEvents="box-none">
      <View style={styles.scrim} pointerEvents="none" />
      <LinearGradient
        colors={['rgba(10,10,12,0.75)', 'rgba(10,10,12,0)']}
        style={styles.scrimTop}
        pointerEvents="none"
      />
      <LinearGradient
        colors={['rgba(10,10,12,0)', 'rgba(10,10,12,0.85)']}
        style={styles.scrimBottom}
        pointerEvents="none"
      />
      <View
        style={[
          styles.topBar,
          {
            paddingTop: insets.top + 4,
            paddingLeft: Math.max(insets.left, spacing.md),
            paddingRight: Math.max(insets.right, spacing.md),
          },
        ]}
        pointerEvents="box-none"
      >
        <Pressable onPress={onBack} hitSlop={12} style={styles.roundButton}>
          <BackIcon />
        </Pressable>
        <View style={styles.titleBox}>
          <Text numberOfLines={1} style={styles.title}>
            {title}
          </Text>
          {sub ? (
            <Text numberOfLines={1} style={styles.subtitle}>
              {sub}
            </Text>
          ) : null}
        </View>
        {onPip ? (
          <Pressable
            onPress={() => {
              onPip();
              poke();
            }}
            hitSlop={12}
            style={styles.roundButton}
          >
            <PipIcon />
          </Pressable>
        ) : null}
        <Pressable
          onPress={() => {
            onOpenSheet();
            poke();
          }}
          hitSlop={12}
          style={styles.roundButton}
        >
          <TracksIcon />
        </Pressable>
      </View>

      <View style={styles.centerRow} pointerEvents="box-none">
        <Pressable
          onPress={() => {
            engine.skip(-10);
            poke();
          }}
          hitSlop={16}
        >
          <Back10Icon />
        </Pressable>
        <Pressable
          onPress={() => {
            engine.togglePlay();
            poke();
          }}
          hitSlop={16}
          style={styles.playButton}
        >
          {engine.playing ? <PauseIcon size={38} /> : <PlayIcon size={38} />}
        </Pressable>
        <Pressable
          onPress={() => {
            engine.skip(10);
            poke();
          }}
          hitSlop={16}
        >
          <Forward10Icon />
        </Pressable>
      </View>

      <View
        style={[
          styles.bottomBar,
          {
            paddingBottom: Math.max(insets.bottom, 12),
            paddingLeft: Math.max(insets.left, spacing.md),
            paddingRight: Math.max(insets.right, spacing.md),
          },
        ]}
      >
        <ScrubRow engine={engine} onInteract={poke} tileFor={tileFor} item={item} />
        <View style={styles.actionsRow}>
          <Pressable
            onPress={() => {
              onOpenSheet();
              poke();
            }}
            hitSlop={10}
            style={styles.actionButton}
          >
            <TracksIcon size={20} />
            <Text style={styles.actionLabel}>{t('player.audioSubShort')}</Text>
          </Pressable>
          {next && onPlayNext ? (
            <Pressable onPress={onPlayNext} hitSlop={10} style={styles.actionButton}>
              <Text style={styles.actionLabel}>{t('player.nextEpisode')}</Text>
              <PlayIcon size={16} />
            </Pressable>
          ) : null}
        </View>
      </View>
    </View>
  );
}

function ScrubRow({
  engine,
  onInteract,
  tileFor,
  item,
}: Readonly<{
  engine: Engine;
  onInteract(): void;
  tileFor?: (abs: number) => StoryboardTile | null;
  item: MediaItem;
}>) {
  const markers = (item.markers ?? []).map((m) => m.startMs / 1000);
  return (
    <View>
      <ScrubBar
        cur={engine.cur}
        dur={engine.dur}
        buffered={engine.buffered}
        tileFor={tileFor}
        markers={markers}
        onSeek={(abs) => {
          engine.seekTo(abs);
          onInteract();
        }}
      />
      <View style={styles.timeRow}>
        <Text style={styles.time}>{formatTimecode(engine.cur)}</Text>
        <Text style={styles.time}>{formatTimecode(engine.dur)}</Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  scrim: { ...absoluteFill, backgroundColor: 'rgba(10, 10, 12, 0.22)' },
  scrimTop: { position: 'absolute', top: 0, left: 0, right: 0, height: 120 },
  scrimBottom: { position: 'absolute', bottom: 0, left: 0, right: 0, height: 160 },
  topBar: { flexDirection: 'row', alignItems: 'center', gap: 4 },
  roundButton: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  titleBox: { flex: 1, alignItems: 'center' },
  title: { color: colors.text, fontSize: 15, fontWeight: '600' },
  subtitle: { color: colors.textDim, fontSize: 12, marginTop: 1 },
  centerRow: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 56,
  },
  playButton: {
    width: 72,
    height: 72,
    borderRadius: 36,
    backgroundColor: 'rgba(244, 243, 240, 0.14)',
    alignItems: 'center',
    justifyContent: 'center',
  },
  bottomBar: { paddingHorizontal: spacing.md },
  actionsRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginTop: 10,
  },
  actionButton: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingVertical: 4 },
  actionLabel: { color: colors.text, fontSize: 13, fontWeight: '600' },
  timeRow: { flexDirection: 'row', justifyContent: 'space-between', marginTop: 2 },
  time: { color: colors.textDim, fontSize: 12, fontVariant: ['tabular-nums'] },
});
