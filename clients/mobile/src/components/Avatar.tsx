// Avatar with the shadcn DX: renders the image when it exists and loads,
// otherwise falls back to the user's initials on a stable per-name gradient.

import { posterColors } from '@kroma/core';
import { Image } from 'expo-image';
import { LinearGradient } from 'expo-linear-gradient';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { colors } from '../lib/theme';

function initialsOf(name: string): string {
  const parts = name
    .trim()
    .split(/[\s._-]+/)
    .filter(Boolean);
  const first = parts[0]?.[0] ?? '?';
  const second = parts.length > 1 ? (parts[parts.length - 1]?.[0] ?? '') : (parts[0]?.[1] ?? '');
  return `${first}${second}`.toUpperCase();
}

export function Avatar({
  uri,
  name,
  size = 40,
}: Readonly<{
  uri: string | null | undefined;
  name: string | null | undefined;
  size?: number;
}>) {
  const [failed, setFailed] = useState(false);
  const label = name?.trim() || '?';
  const [from, to] = posterColors(label);
  const showImage = !!uri && !failed;
  return (
    <View style={[styles.box, { width: size, height: size, borderRadius: size / 2 }]}>
      <LinearGradient colors={[from, to]} style={StyleSheet.absoluteFill} />
      {!showImage ? (
        <Text style={[styles.initials, { fontSize: size * 0.38 }]}>{initialsOf(label)}</Text>
      ) : (
        <Image
          source={{ uri }}
          contentFit="cover"
          transition={200}
          cachePolicy="memory-disk"
          style={StyleSheet.absoluteFill}
          onError={() => setFailed(true)}
        />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  box: {
    overflow: 'hidden',
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: colors.surfaceRaised,
  },
  initials: { color: colors.text, fontWeight: '700', letterSpacing: 0.5 },
});
