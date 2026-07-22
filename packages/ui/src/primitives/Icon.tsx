// <Icon> on the native targets (Apple TV, Android TV): react-native-svg.
//
// Same glyph data as the web renderer (icons.generated.ts); only the element
// factory differs. The generator refuses any Tabler glyph using an element that
// is not in this map, so a missing case is a build error, never a silently
// half-drawn icon.

import type { ComponentType } from 'react';
import Svg, { Circle, Ellipse, Line, Path, Polygon, Polyline, Rect } from 'react-native-svg';
import { type IconProps, resolveIcon } from './icons/glyph';

export type { IconName, IconProps } from './icons/glyph';

const ELEMENTS: Record<string, ComponentType<Record<string, unknown>>> = {
  path: Path as ComponentType<Record<string, unknown>>,
  circle: Circle as ComponentType<Record<string, unknown>>,
  rect: Rect as ComponentType<Record<string, unknown>>,
  line: Line as ComponentType<Record<string, unknown>>,
  polyline: Polyline as ComponentType<Record<string, unknown>>,
  polygon: Polygon as ComponentType<Record<string, unknown>>,
  ellipse: Ellipse as ComponentType<Record<string, unknown>>,
};

export function Icon(props: Readonly<IconProps>) {
  const { glyph, size, viewBox, root } = resolveIcon(props);
  return (
    <Svg
      width={size}
      height={size}
      viewBox={viewBox}
      fill={root.fill}
      stroke={root.stroke}
      strokeWidth={root.strokeWidth}
      strokeLinecap={root.strokeLinecap}
      strokeLinejoin={root.strokeLinejoin}
    >
      {glyph.nodes.map(([tag, attrs], i) => {
        const El = ELEMENTS[tag];
        // biome-ignore lint/suspicious/noArrayIndexKey: a glyph's node list is generated, fixed-length and never reordered.
        return El ? <El key={i} {...attrs} /> : null;
      })}
    </Svg>
  );
}
