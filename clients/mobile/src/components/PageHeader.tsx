// Shared page header: back chevron, centered title, optional right slot.
// The surrounding <Screen> already pads the top safe area (Dynamic Island /
// status bar), so the header adds only its own breathing room.

import { useRouter } from 'expo-router';
import type { ReactNode } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { spacing, type } from '../lib/theme';
import { BackIcon } from '../player/icons';

export function PageHeader({ title, right }: Readonly<{ title: string; right?: ReactNode }>) {
  const router = useRouter();
  return (
    <View style={[styles.header, { paddingTop: 6 }]}>
      <Pressable onPress={() => router.back()} hitSlop={12} style={styles.side}>
        <BackIcon />
      </Pressable>
      <Text numberOfLines={1} style={styles.title}>
        {title}
      </Text>
      <View style={styles.side}>{right}</View>
    </View>
  );
}

const styles = StyleSheet.create({
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: spacing.md,
    paddingBottom: spacing.sm,
    gap: spacing.sm,
  },
  side: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  title: { ...type.heading, flex: 1, textAlign: 'center' },
});
