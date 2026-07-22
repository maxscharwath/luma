// The 10-foot stage.
//
// Every TV screen is authored against a fixed 1920x1080 canvas, and this
// component scales that canvas to fit whatever the platform actually gives us.
// It is what makes the layout PIXEL-IDENTICAL across the four targets, whose
// native units disagree wildly:
//
//   Tizen / webOS   1920x1080 CSS px on a real panel, anything in a dev browser
//   Apple TV        1920x1080 points (the same on a 4K set: tvOS scales @2x)
//   Android TV      960x540 dp at density 2.0 on a 1080p panel
//
// Without this, an Android TV would render the whole design at double size.
// With it, one set of numbers is correct everywhere and the design never has to
// be re-tuned per platform.

import type { ReactNode } from 'react';
import { StyleSheet, useWindowDimensions, View } from 'react-native';
import { CANVAS, colors } from '../tokens';

export interface TvStageProps {
  children: ReactNode;
}

export function TvStage({ children }: Readonly<TvStageProps>) {
  const { width, height } = useWindowDimensions();
  // Contain, never cover: a letterboxed stage is correct on an unusual aspect
  // ratio, a cropped one loses the overscan-safe gutters.
  const scale = Math.min(width / CANVAS.width, height / CANVAS.height);

  return (
    <View style={styles.viewport}>
      {/* The canvas is centred in the viewport, and React Native scales around
          an element's centre, so no transform-origin juggling is needed. */}
      <View style={[styles.canvas, { transform: [{ scale }] }]}>{children}</View>
    </View>
  );
}

const styles = StyleSheet.create({
  viewport: {
    flex: 1,
    backgroundColor: colors.bg,
    alignItems: 'center',
    justifyContent: 'center',
    overflow: 'hidden',
  },
  canvas: {
    width: CANVAS.width,
    height: CANVAS.height,
    backgroundColor: colors.bg,
    overflow: 'hidden',
  },
});
