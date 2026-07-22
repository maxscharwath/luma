// Touch scrub bar: buffered + played fills, drag preview with a storyboard
// thumbnail + time bubble. A plain tap seeks directly; the engine decides
// native seek vs re-anchor on commit.

import { formatTimecode } from '@kroma/core';
import { Image } from 'expo-image';
import { useRef, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import { colors, radius } from '../lib/theme';
import type { StoryboardTile } from './useStoryboard';

const BAR_H = 3;
const BAR_H_ACTIVE = 5;
const THUMB_W = 148;

export function ScrubBar({
  cur,
  dur,
  buffered,
  onSeek,
  tileFor,
  markers,
}: Readonly<{
  cur: number;
  dur: number;
  buffered: number;
  onSeek(abs: number): void;
  tileFor?: (abs: number) => StoryboardTile | null;
  /** Chapter/marker starts (abs seconds) shown as ticks on the track. */
  markers?: number[];
}>) {
  const [preview, setPreview] = useState<number | null>(null);
  const [width, setWidth] = useState(1);
  const widthRef = useRef(1);
  const durRef = useRef(dur);
  durRef.current = dur;

  const toTime = (x: number) =>
    Math.max(0, Math.min(durRef.current, (x / widthRef.current) * durRef.current));

  const pan = Gesture.Pan()
    .runOnJS(true)
    .activeOffsetX([-5, 5])
    .onBegin((e) => setPreview(toTime(e.x)))
    .onUpdate((e) => setPreview(toTime(e.x)))
    .onEnd((e) => {
      onSeek(toTime(e.x));
      setPreview(null);
    })
    .onFinalize(() => setPreview(null));
  const tap = Gesture.Tap()
    .runOnJS(true)
    .onEnd((e, ok) => {
      if (ok) onSeek(toTime(e.x));
      setPreview(null);
    });
  const gesture = Gesture.Race(pan, tap);

  const shown = preview ?? cur;
  const playedFrac = dur > 0 ? Math.min(1, shown / dur) : 0;
  const bufFrac = dur > 0 ? Math.min(1, buffered / dur) : 0;
  const active = preview != null;
  const h = active ? BAR_H_ACTIVE : BAR_H;
  const tile = active && tileFor ? tileFor(shown) : null;
  const thumbH = tile ? Math.round((THUMB_W / tile.tileW) * tile.tileH) : 0;
  const scale = tile ? THUMB_W / tile.tileW : 1;
  const previewLeft = Math.max(THUMB_W / 2, Math.min(width - THUMB_W / 2, playedFrac * width));

  return (
    <GestureDetector gesture={gesture}>
      <View
        style={styles.touch}
        onLayout={(e) => {
          widthRef.current = e.nativeEvent.layout.width;
          setWidth(e.nativeEvent.layout.width);
        }}
      >
        {active ? (
          <View style={[styles.previewBox, { left: previewLeft - THUMB_W / 2 }]}>
            {tile ? (
              <View style={[styles.thumb, { width: THUMB_W, height: thumbH }]}>
                <Image
                  source={{ uri: tile.sheet }}
                  contentFit="fill"
                  style={{
                    position: 'absolute',
                    left: -tile.x * scale,
                    top: -tile.y * scale,
                    width: tile.sheetW * scale,
                    height: tile.sheetH * scale,
                  }}
                />
              </View>
            ) : null}
            <View style={styles.bubble}>
              <Text style={styles.bubbleText}>{formatTimecode(shown)}</Text>
            </View>
          </View>
        ) : null}
        <View style={[styles.track, { height: h }]}>
          <View style={[styles.buffered, { width: `${bufFrac * 100}%`, height: h }]} />
          <View style={[styles.played, { width: `${playedFrac * 100}%`, height: h }]} />
          {dur > 0
            ? (markers ?? [])
                .filter((m) => m > 0 && m < dur)
                .map((m) => (
                  <View key={m} style={[styles.markerTick, { left: `${(m / dur) * 100}%` }]} />
                ))
            : null}
        </View>
        <View style={[styles.knob, { left: playedFrac * width - 6, opacity: active ? 1 : 0 }]} />
      </View>
    </GestureDetector>
  );
}

const styles = StyleSheet.create({
  touch: { height: 36, justifyContent: 'center' },
  track: {
    borderRadius: 3,
    backgroundColor: 'rgba(244, 243, 240, 0.25)',
    overflow: 'hidden',
  },
  buffered: {
    position: 'absolute',
    left: 0,
    backgroundColor: 'rgba(244, 243, 240, 0.35)',
  },
  played: { position: 'absolute', left: 0, backgroundColor: colors.accent },
  markerTick: {
    position: 'absolute',
    top: 0,
    bottom: 0,
    width: 2.5,
    backgroundColor: 'rgba(10, 10, 12, 0.9)',
  },
  knob: {
    position: 'absolute',
    width: 12,
    height: 12,
    borderRadius: 6,
    backgroundColor: colors.accent,
  },
  previewBox: {
    position: 'absolute',
    bottom: 34,
    width: THUMB_W,
    alignItems: 'center',
    gap: 4,
  },
  thumb: {
    borderRadius: radius.sm,
    overflow: 'hidden',
    borderWidth: 1.5,
    borderColor: 'rgba(244, 243, 240, 0.6)',
    backgroundColor: '#000',
  },
  bubble: {
    borderRadius: 6,
    backgroundColor: 'rgba(10, 10, 12, 0.85)',
    paddingHorizontal: 8,
    paddingVertical: 2,
  },
  bubbleText: { color: colors.text, fontSize: 11, fontVariant: ['tabular-nums'] },
});
