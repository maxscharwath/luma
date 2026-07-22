// <Box>: the layout primitive.
//
// A React Native <View> that takes the design's vocabulary directly, so a screen
// reads as layout instead of as a StyleSheet lookup table:
//
//   <Box row center gap={12} px={64} py={24} bg="surface1" radius="lg" flex>
//
// `style` is still there and always wins, for the genuinely one-off cases.

import type { ReactNode } from 'react';
import { View, type ViewProps } from 'react-native';
import { type BoxStyleProps, boxStyle } from './boxStyle';

export interface BoxProps extends BoxStyleProps, Omit<ViewProps, 'style'> {
  children?: ReactNode;
  style?: ViewProps['style'];
}

export function Box({ children, style, ...props }: Readonly<BoxProps>) {
  const { view, layout } = splitProps(props);
  return (
    <View {...view} style={[layout, style]}>
      {children}
    </View>
  );
}

/** Horizontal <Box>. Sugar for the single most common case, and it reads better
 * at a call site than `row` buried among a dozen other props. */
export function Row({ children, ...props }: Readonly<BoxProps>) {
  return (
    <Box row align="center" {...props}>
      {children}
    </Box>
  );
}

/** Vertical <Box>. React Native already stacks in a column, so this exists for
 * symmetry with <Row> and to make intent explicit at the call site. */
export function Column({ children, ...props }: Readonly<BoxProps>) {
  return <Box {...props}>{children}</Box>;
}

/** Pushes whatever follows it to the far end of a <Row>. */
export function Spacer() {
  return <View style={SPACER} />;
}

const SPACER = { flex: 1 } as const;

/** Every style shorthand <Box> owns. Anything else is a real View prop and is
 * forwarded untouched (onLayout, pointerEvents, testID, accessibility...). */
const STYLE_PROPS = new Set([
  'flex',
  'row',
  'wrap',
  'center',
  'align',
  'justify',
  'self',
  'shrink',
  'grow',
  'gap',
  'between',
  'w',
  'h',
  'minW',
  'minH',
  'maxW',
  'maxH',
  'aspect',
  'fill',
  'absolute',
  'top',
  'right',
  'bottom',
  'left',
  'z',
  'p',
  'px',
  'py',
  'pt',
  'pr',
  'pb',
  'pl',
  'm',
  'mx',
  'my',
  'mt',
  'mr',
  'mb',
  'ml',
  'bg',
  'radius',
  'border',
  'borderWidth',
  'shadow',
  'opacity',
  'overflow',
]);

function splitProps(props: Record<string, unknown>): {
  view: Record<string, unknown>;
  layout: ReturnType<typeof boxStyle>;
} {
  const style: Record<string, unknown> = {};
  const view: Record<string, unknown> = {};
  for (const key of Object.keys(props)) {
    if (STYLE_PROPS.has(key)) style[key] = props[key];
    else view[key] = props[key];
  }
  return { view, layout: boxStyle(style as BoxStyleProps) };
}
