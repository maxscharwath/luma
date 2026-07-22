// The player overlay shell: the tap surface that shows and hides the controls
// (tap to toggle, double-tap the screen edges to skip 10s) and the overlays that
// stay up regardless. Pure presentation over the Engine; all playback logic
// lives in engine/.

import type { MediaItem } from '@kroma/core';
import * as Haptics from 'expo-haptics';
import { useEffect, useRef, useState } from 'react';
import { StyleSheet, useWindowDimensions, View } from 'react-native';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { ControlsLayer } from './chrome/ControlsLayer';
import { BufferingSpinner, CueLine, SkipIntroButton, UpNextCard } from './chrome/overlays';
import type { Engine } from './engine';
import type { StoryboardTile } from './useStoryboard';

const HIDE_AFTER_MS = 4000;

export function PlayerChrome({
  engine,
  item,
  cue,
  onBack,
  onOpenSheet,
  tileFor,
  next,
  onPlayNext,
  onPip,
}: Readonly<{
  engine: Engine;
  item: MediaItem;
  cue: string;
  onBack(): void;
  onOpenSheet(): void;
  tileFor?: (abs: number) => StoryboardTile | null;
  next?: MediaItem | null;
  onPlayNext?(): void;
  onPip?(): void;
}>) {
  const insets = useSafeAreaInsets();
  const [visible, setVisible] = useState(true);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const poke = () => {
    setVisible(true);
    if (hideTimer.current) clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setVisible(false), HIDE_AFTER_MS);
  };

  // biome-ignore lint/correctness/useExhaustiveDependencies: arm the auto-hide once on mount
  useEffect(() => {
    poke();
    return () => {
      if (hideTimer.current) clearTimeout(hideTimer.current);
    };
  }, []);

  const { width: screenWidth } = useWindowDimensions();
  const tap = Gesture.Tap()
    .runOnJS(true)
    .onEnd(() => {
      if (visible) setVisible(false);
      else poke();
    });
  const doubleTap = Gesture.Tap()
    .runOnJS(true)
    .numberOfTaps(2)
    .onEnd((e, ok) => {
      if (!ok) return;
      // Left third rewinds, right third fast-forwards, middle is ignored.
      if (e.x < screenWidth / 3) engine.skip(-10);
      else if (e.x > (screenWidth * 2) / 3) engine.skip(10);
      else return;
      void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
      poke();
    });
  const gestures = Gesture.Exclusive(doubleTap, tap);

  const intro = (item.markers ?? []).find(
    (m) => m.kind === 'intro' && engine.cur * 1000 >= m.startMs && engine.cur * 1000 < m.endMs,
  );
  const inCredits = (item.markers ?? []).some(
    (m) => m.kind === 'credits' && engine.cur * 1000 >= m.startMs,
  );

  return (
    <View style={StyleSheet.absoluteFill}>
      {/* Tap layer sits BEHIND the controls: toggling visibility must not fire
          when a control is pressed. */}
      <GestureDetector gesture={gestures}>
        <View style={StyleSheet.absoluteFill} />
      </GestureDetector>
      <View style={StyleSheet.absoluteFill} pointerEvents="box-none">
        <CueLine cue={cue} bottom={(visible ? 110 : 40) + insets.bottom} />

        {engine.waiting && !engine.failed ? <BufferingSpinner /> : null}

        {intro && !visible ? (
          <SkipIntroButton
            onPress={() => engine.seekTo(intro.endMs / 1000)}
            bottom={40 + insets.bottom}
          />
        ) : null}

        {inCredits && next && onPlayNext ? (
          <UpNextCard next={next} onPlayNext={onPlayNext} bottom={40 + insets.bottom} />
        ) : null}

        {visible ? (
          <ControlsLayer
            engine={engine}
            item={item}
            insets={insets}
            poke={poke}
            onBack={onBack}
            onOpenSheet={onOpenSheet}
            tileFor={tileFor}
            next={next}
            onPlayNext={onPlayNext}
            onPip={onPip}
          />
        ) : null}
      </View>
    </View>
  );
}
