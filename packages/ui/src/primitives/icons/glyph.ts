// Shared <Icon> logic: everything about a glyph that is not "which element
// factory do I call". Both renderers derive their root attributes from here, so
// stroke weight, fill rules and sizing can never drift between platforms.

import { type ColorToken, colors } from '../../tokens';
import { ICON_VIEWBOX, ICONS, type IconGlyph, type IconName } from './icons.generated';

export type { IconName, IconNode } from './icons.generated';

export interface IconProps {
  name: IconName;
  /** Rendered size in px on the 1920x1080 design canvas. Default 24, Tabler's
   *  native grid, so the default needs no scaling at all. */
  size?: number;
  /** A palette token, or any raw colour string. Defaults to the body text colour
   *  because React Native has no `currentColor` to inherit. */
  color?: ColorToken | (string & {});
  /** Outline weight. Tabler draws at 2; the design thins it to 1.8 for the
   *  player transport. Ignored by filled glyphs. */
  stroke?: number;
}

export interface ResolvedIcon {
  glyph: IconGlyph;
  size: number;
  viewBox: string;
  /** Root attributes: filled glyphs paint, outline glyphs stroke. */
  root: {
    fill: string;
    stroke: string;
    strokeWidth: number;
    strokeLinecap: 'round';
    strokeLinejoin: 'round';
  };
}

export const DEFAULT_ICON_SIZE = 24;
export const DEFAULT_ICON_STROKE = 2;

export function resolveIcon({
  name,
  size = DEFAULT_ICON_SIZE,
  color = 'text',
  stroke = DEFAULT_ICON_STROKE,
}: Readonly<IconProps>): ResolvedIcon {
  const glyph = ICONS[name];
  const paint = (colors as Record<string, string>)[color] ?? color;
  return {
    glyph,
    size,
    viewBox: `0 0 ${ICON_VIEWBOX} ${ICON_VIEWBOX}`,
    root: glyph.filled
      ? {
          fill: paint,
          stroke: 'none',
          strokeWidth: 0,
          strokeLinecap: 'round',
          strokeLinejoin: 'round',
        }
      : {
          fill: 'none',
          stroke: paint,
          strokeWidth: stroke,
          strokeLinecap: 'round',
          strokeLinejoin: 'round',
        },
  };
}

/** Every icon name, for tests and for a gallery screen. */
export const ICON_NAMES = Object.keys(ICONS) as IconName[];
