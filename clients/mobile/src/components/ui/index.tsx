// Small shared primitives for the mobile screens: the dark screen scaffold and
// text surfaces, re-exporting the controls and state views so every screen keeps
// importing them from one place.

import { type ReactNode, useState } from 'react';
import { Pressable, StyleSheet, Text, View, type ViewStyle } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useT } from '../../lib/i18n';
import { colors, spacing, type } from '../../lib/theme';

export { Button, Chip, TextField } from './controls';
export { EmptyState, ErrorBanner, ErrorView, Loading } from './states';

export function Screen({
  children,
  padded = true,
  style,
}: Readonly<{
  children: ReactNode;
  padded?: boolean;
  style?: ViewStyle;
}>) {
  const insets = useSafeAreaInsets();
  return (
    <View
      style={[
        styles.screen,
        { paddingTop: insets.top },
        padded && { paddingHorizontal: spacing.md },
        style,
      ]}
    >
      {children}
    </View>
  );
}

/** Netflix-style collapsed paragraph: clamped with a "more" toggle. A clamped
 * Text reports the CLAMPED count in onTextLayout, so the real line count is
 * measured on a hidden unclamped copy rendered behind the visible one. */
export function ExpandableText({
  children,
  lines = 3,
}: Readonly<{ children: string; lines?: number }>) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const [clampable, setClampable] = useState(false);
  return (
    <Pressable onPress={() => clampable && setExpanded((v) => !v)}>
      <Text style={styles.expandable} numberOfLines={expanded ? undefined : lines}>
        {children}
      </Text>
      <Text
        accessible={false}
        style={[styles.expandable, styles.expandGhost]}
        onTextLayout={(e) => setClampable(e.nativeEvent.lines.length > lines)}
      >
        {children}
      </Text>
      {clampable && !expanded ? (
        <Text style={styles.expandMore}>{`… ${t('content.moreInfo')}`}</Text>
      ) : null}
    </Pressable>
  );
}

export function SectionTitle({ children }: Readonly<{ children: ReactNode }>) {
  return <Text style={styles.sectionTitle}>{children}</Text>;
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.bg },
  expandable: { ...type.body, color: colors.textDim, lineHeight: 22 },
  expandMore: { ...type.caption, color: colors.text, fontWeight: '700', marginTop: 2 },
  expandGhost: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 0,
    opacity: 0,
    pointerEvents: 'none',
  },
  sectionTitle: {
    ...type.section,
    marginTop: spacing.lg,
    marginBottom: spacing.sm,
    paddingHorizontal: spacing.md,
  },
});
