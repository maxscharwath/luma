// Native counterpart of the @kroma/ui <Image> fade primitive: artwork fades in
// on load and cross-fades on source change (expo-image does both natively),
// over the same per-title gradient placeholder the other clients use.

import { posterColors } from '@kroma/core';
import { Image, type ImageContentFit } from 'expo-image';
import { LinearGradient } from 'expo-linear-gradient';
import type { StyleProp, ViewStyle } from 'react-native';
import { StyleSheet, View } from 'react-native';

export interface FadeImageProps {
  uri: string | null;
  /** Seed for the placeholder gradient (item id keeps it stable per title). */
  seed?: string;
  fit?: ImageContentFit;
  radius?: number;
  style?: StyleProp<ViewStyle>;
}

export function FadeImage({
  uri,
  seed,
  fit = 'cover',
  radius = 0,
  style,
}: Readonly<FadeImageProps>) {
  const [from, to] = posterColors(seed ?? uri ?? 'kroma');
  return (
    <View style={[styles.box, { borderRadius: radius }, style]}>
      <LinearGradient colors={[from, to]} style={StyleSheet.absoluteFill} />
      {uri ? (
        <Image
          source={{ uri }}
          contentFit={fit}
          transition={250}
          cachePolicy="memory-disk"
          style={StyleSheet.absoluteFill}
          recyclingKey={uri}
        />
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  box: { overflow: 'hidden', backgroundColor: '#141418' },
});
