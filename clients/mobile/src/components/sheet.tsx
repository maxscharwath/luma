// Shared bottom-sheet primitives (@gorhom/bottom-sheet): a dark, dynamically
// sized modal sheet with backdrop + grab handle, plus row/title building
// blocks. Used for the season picker and any future action sheets so every
// sheet in the app feels identical.

import {
  BottomSheetBackdrop,
  type BottomSheetBackdropProps,
  BottomSheetModal,
  BottomSheetView,
} from '@gorhom/bottom-sheet';
import { forwardRef, type ReactNode, useCallback } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { colors, radius, spacing, type } from '../lib/theme';
import { CheckIcon } from '../player/icons';

export type AppSheetRef = BottomSheetModal;

export const AppSheet = forwardRef<BottomSheetModal, { children: ReactNode }>(function AppSheet(
  { children },
  ref,
) {
  const insets = useSafeAreaInsets();
  const renderBackdrop = useCallback(
    (props: BottomSheetBackdropProps) => (
      <BottomSheetBackdrop {...props} appearsOnIndex={0} disappearsOnIndex={-1} opacity={0.6} />
    ),
    [],
  );
  return (
    <BottomSheetModal
      ref={ref}
      enableDynamicSizing
      backdropComponent={renderBackdrop}
      backgroundStyle={styles.background}
      handleIndicatorStyle={styles.handle}
    >
      <BottomSheetView
        style={[styles.content, { paddingBottom: Math.max(insets.bottom, spacing.md) }]}
      >
        {children}
      </BottomSheetView>
    </BottomSheetModal>
  );
});

export function SheetTitle({ children }: Readonly<{ children: ReactNode }>) {
  return <Text style={styles.title}>{children}</Text>;
}

export function SheetRow({
  label,
  detail,
  active,
  onPress,
}: Readonly<{
  label: string;
  detail?: string;
  active?: boolean;
  onPress(): void;
}>) {
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [styles.row, pressed && { backgroundColor: colors.surfaceHigh }]}
    >
      <Text style={[styles.rowLabel, active && { color: colors.accent, fontWeight: '800' }]}>
        {label}
      </Text>
      <View style={styles.rowRight}>
        {detail ? <Text style={styles.rowDetail}>{detail}</Text> : null}
        {active ? <CheckIcon size={17} color={colors.accent} /> : null}
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  background: {
    backgroundColor: colors.surfaceRaised,
    borderTopLeftRadius: radius.xl,
    borderTopRightRadius: radius.xl,
  },
  handle: { backgroundColor: colors.textFaint, width: 40 },
  content: { paddingHorizontal: spacing.sm, paddingTop: spacing.xs },
  title: {
    ...type.small,
    textTransform: 'uppercase',
    letterSpacing: 1,
    paddingHorizontal: spacing.sm,
    marginBottom: spacing.xs,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    minHeight: 50,
    paddingHorizontal: spacing.md,
    borderRadius: radius.md,
    gap: spacing.md,
  },
  rowLabel: { ...type.body, color: colors.text, fontWeight: '500', flexShrink: 1 },
  rowRight: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  rowDetail: { ...type.small },
});
