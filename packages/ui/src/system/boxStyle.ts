// The <Box> shorthand resolver: prop bag in, one ViewStyle out.
//
// Pure and separate from the component so the mapping is unit-tested rather than
// eyeballed, and so <Box> itself stays a five-line render.
//
// Sizes are plain numbers, deliberately. Every TV screen is authored on the
// fixed 1920x1080 canvas (see <TvStage>), so a number IS the design's px value:
// no scale to memorise, and it matches how the design specifies arbitrary values
// (px-9, gap-2.75, text-[19px]) rather than pretending they sit on a 4pt grid.
// Only the values that ARE tokens (colour, radius, elevation) take token names.

import type { ViewStyle } from 'react-native';
import { absoluteFill, type ColorToken, colors, radius, shadow } from '../tokens';

export type Spacing = number;
export type Align = ViewStyle['alignItems'];
export type Justify = ViewStyle['justifyContent'];

export interface BoxStyleProps {
  // ---- flex ----
  /** `flex: 1` when true, or the explicit flex factor. */
  flex?: boolean | number;
  /** Lay children out horizontally (default is React Native's column). */
  row?: boolean;
  wrap?: boolean;
  /** Centre on BOTH axes: the single most common layout in a 10-foot UI. */
  center?: boolean;
  align?: Align;
  justify?: Justify;
  /** Override the parent's alignment for this child alone. */
  self?: ViewStyle['alignSelf'];
  /** `flexShrink`. Rail tiles set 0 so they keep their width in a row. */
  shrink?: number;
  grow?: number;
  gap?: Spacing;
  /** Space children evenly with the container's ends flush. */
  between?: boolean;

  // ---- box ----
  w?: ViewStyle['width'];
  h?: ViewStyle['height'];
  minW?: ViewStyle['minWidth'];
  minH?: ViewStyle['minHeight'];
  maxW?: ViewStyle['maxWidth'];
  maxH?: ViewStyle['maxHeight'];
  aspect?: number;

  // ---- position ----
  /** Stretch to the positioned parent (absolute, inset 0). */
  fill?: boolean;
  absolute?: boolean;
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
  z?: number;

  // ---- spacing ----
  p?: Spacing;
  px?: Spacing;
  py?: Spacing;
  pt?: Spacing;
  pr?: Spacing;
  pb?: Spacing;
  pl?: Spacing;
  m?: Spacing;
  mx?: Spacing;
  my?: Spacing;
  mt?: Spacing;
  mr?: Spacing;
  mb?: Spacing;
  ml?: Spacing;

  // ---- paint ----
  /** A palette token name, or any raw colour string for one-offs. */
  bg?: ColorToken | (string & {});
  radius?: keyof typeof radius | number;
  border?: ColorToken | (string & {});
  borderWidth?: number;
  /** Elevation token. */
  shadow?: keyof typeof shadow;
  opacity?: number;
  overflow?: ViewStyle['overflow'];
}

/** A token name resolves through the palette; anything else is a raw colour. */
export function color(value: string): string {
  return (colors as Record<string, string>)[value] ?? value;
}

function radiusOf(value: keyof typeof radius | number): number {
  return typeof value === 'number' ? value : radius[value];
}

/** Assign only the defined entries, so `<Box />` produces an empty style rather
 * than a bag of `undefined`s that React Native would still have to diff. */
function put(out: Record<string, unknown>, key: string, value: unknown): void {
  if (value !== undefined) out[key] = value;
}

export function boxStyle(p: Readonly<BoxStyleProps>): ViewStyle {
  const out: Record<string, unknown> = p.fill ? { ...absoluteFill } : {};

  if (p.flex === true) out.flex = 1;
  else put(out, 'flex', p.flex);
  if (p.row) out.flexDirection = 'row';
  if (p.wrap) out.flexWrap = 'wrap';
  if (p.center) {
    out.alignItems = 'center';
    out.justifyContent = 'center';
  }
  if (p.between) out.justifyContent = 'space-between';
  put(out, 'alignItems', p.align);
  put(out, 'justifyContent', p.justify);
  put(out, 'alignSelf', p.self);
  put(out, 'flexShrink', p.shrink);
  put(out, 'flexGrow', p.grow);
  put(out, 'gap', p.gap);

  put(out, 'width', p.w);
  put(out, 'height', p.h);
  put(out, 'minWidth', p.minW);
  put(out, 'minHeight', p.minH);
  put(out, 'maxWidth', p.maxW);
  put(out, 'maxHeight', p.maxH);
  put(out, 'aspectRatio', p.aspect);

  if (p.absolute) out.position = 'absolute';
  put(out, 'top', p.top);
  put(out, 'right', p.right);
  put(out, 'bottom', p.bottom);
  put(out, 'left', p.left);
  put(out, 'zIndex', p.z);

  // Longhands only, never the `padding`/`margin` shorthand: React Native
  // resolves shorthand vs longhand by declaration order inside one object, so a
  // `{ padding, paddingTop }` pair is order-dependent and surprising.
  putEdges(out, 'padding', p.p, p.px, p.py, p.pt, p.pr, p.pb, p.pl);
  putEdges(out, 'margin', p.m, p.mx, p.my, p.mt, p.mr, p.mb, p.ml);

  if (p.bg !== undefined) out.backgroundColor = color(p.bg);
  if (p.radius !== undefined) out.borderRadius = radiusOf(p.radius);
  if (p.border !== undefined) {
    out.borderColor = color(p.border);
    out.borderWidth = p.borderWidth ?? 1;
  } else {
    put(out, 'borderWidth', p.borderWidth);
  }
  if (p.shadow !== undefined) out.boxShadow = shadow[p.shadow];
  put(out, 'opacity', p.opacity);
  put(out, 'overflow', p.overflow);

  return out as ViewStyle;
}

/** Expand all / axis / side into the four longhands, most specific winning. */
function putEdges(
  out: Record<string, unknown>,
  prefix: 'padding' | 'margin',
  all?: number,
  x?: number,
  y?: number,
  top?: number,
  right?: number,
  bottom?: number,
  left?: number,
): void {
  const t = top ?? y ?? all;
  const r = right ?? x ?? all;
  const b = bottom ?? y ?? all;
  const l = left ?? x ?? all;
  put(out, `${prefix}Top`, t);
  put(out, `${prefix}Right`, r);
  put(out, `${prefix}Bottom`, b);
  put(out, `${prefix}Left`, l);
}
