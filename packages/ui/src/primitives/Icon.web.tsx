// <Icon> on the browser targets: a plain DOM <svg>.
//
// Deliberately NOT react-native-svg here. react-native-svg does work under
// react-native-web, but it drags a large runtime through the bundler for
// something the browser already does natively, and every byte counts on a TV's
// slow connection. The glyph data is the same on both sides (icons.generated.ts),
// so only the element factory differs.

import { createElement } from 'react';
import { type IconProps, resolveIcon } from './icons/glyph';

export type { IconName, IconProps } from './icons/glyph';

export function Icon(props: Readonly<IconProps>) {
  const { glyph, size, viewBox, root } = resolveIcon(props);
  return (
    <svg
      width={size}
      height={size}
      viewBox={viewBox}
      fill={root.fill}
      stroke={root.stroke}
      strokeWidth={root.strokeWidth}
      strokeLinecap={root.strokeLinecap}
      strokeLinejoin={root.strokeLinejoin}
      aria-hidden="true"
      focusable="false"
      // An <svg> is an inline element, so without this it sits on the text
      // baseline and adds descender space inside a flex row.
      style={{ display: 'block', flexShrink: 0 }}
    >
      {glyph.nodes.map(([tag, attrs], i) => createElement(tag, { ...attrs, key: i }))}
    </svg>
  );
}
