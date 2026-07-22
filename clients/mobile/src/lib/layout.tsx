// Tablet-aware layout primitives. Width caps are plain yoga styles (maxWidth
// only bites once the window is wider than the cap), so phones pass through
// untouched. Multi-column layouts key off the LIVE window width (useIsWide),
// never the device class: iPadOS windows resize freely, and a narrow floating
// window must collapse back to the single-column phone flow.

import * as Device from 'expo-device';
import type { ReactNode } from 'react';
import {
  type StyleProp,
  StyleSheet,
  useWindowDimensions,
  View,
  type ViewStyle,
} from 'react-native';
import { spacing } from './theme';

/** Device class, for capabilities that genuinely follow the hardware (e.g.
 * player orientation locks). Layout decisions should use useIsWide instead. */
export const isTablet = Device.deviceType === Device.DeviceType.TABLET;

/** Window width from which multi-column layouts engage. Full-screen iPads
 * pass in both orientations; iPhones and narrow iPad windows never do. */
export const WIDE_BREAKPOINT = 700;

/** True while the window is wide enough for multi-column layouts; re-renders
 * on window resize (iPad windowing, split view, rotation). */
export function useIsWide(min: number = WIDE_BREAKPOINT): boolean {
  const { width } = useWindowDimensions();
  return width >= min;
}

/** Shared content-column caps (pt): one vocabulary instead of magic numbers. */
export const contentWidth = {
  /** Single form column: sign-in, add-server, profile gate. */
  form: 480,
  /** Reading column: settings lists, prose, single-column detail bodies. */
  reading: 720,
} as const;

/** Centered-column cap as a bare style, for style arrays on existing views. */
export function boxed(max: number = contentWidth.form): ViewStyle {
  return { width: '100%', maxWidth: max, alignSelf: 'center' };
}

/** Caps children to a centered column. Wrap any screen body in it; on phones
 * it is a no-op because the cap never engages. */
export function MaxWidth({
  max,
  style,
  children,
}: Readonly<{
  max?: number;
  style?: StyleProp<ViewStyle>;
  children: ReactNode;
}>) {
  return <View style={[boxed(max), style]}>{children}</View>;
}

/** Side-by-side columns in wide windows; below the breakpoint, left and right
 * render as plain stacked siblings inside `style`, so the narrow layout stays
 * byte-identical when a page adopts this. Collapses live on window resize. */
export function SplitColumns({
  left,
  right,
  leftFlex = 2,
  rightFlex = 3,
  style,
}: Readonly<{
  left: ReactNode;
  right: ReactNode;
  leftFlex?: number;
  rightFlex?: number;
  style?: StyleProp<ViewStyle>;
}>) {
  const wide = useIsWide();
  if (!wide) {
    return (
      <View style={style}>
        {left}
        {right}
      </View>
    );
  }
  return (
    <View style={[style, styles.splitRow]}>
      <View style={[styles.splitCol, { flex: leftFlex }]}>{left}</View>
      <View style={[styles.splitCol, { flex: rightFlex }]}>{right}</View>
    </View>
  );
}

const styles = StyleSheet.create({
  splitRow: { flexDirection: 'row', alignItems: 'flex-start', gap: spacing.lg },
  splitCol: { gap: spacing.md },
});
