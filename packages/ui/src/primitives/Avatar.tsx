// <Avatar>: the profile / cast disc. A photo when there is one, otherwise a
// gradient with the initials, so a profile is never a blank circle.

import { Box } from '../system/Box';
import { fonts, radius } from '../tokens';
import { Img } from './Img';
import { Txt } from './Text';

/** The brand's warm default, used when the caller has no per-profile gradient. */
export const AVATAR_GRADIENT = 'linear-gradient(135deg, #F4B642, #E8743B)';

export interface AvatarProps {
  name?: string;
  /** Photo URL. Falls back to the initials on error or when absent. */
  src?: string | null;
  size?: number;
  /** CSS gradient behind the initials. */
  gradient?: string;
  /** Corner radius. Defaults to a full circle. */
  radius?: number;
}

/** First letter of the first two words, e.g. "Marie Curie" -> "MC". */
export function initialsOf(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => word[0] ?? '')
    .join('')
    .toUpperCase();
}

export function Avatar({
  name = '',
  src = null,
  size = 64,
  gradient = AVATAR_GRADIENT,
  radius: corner,
}: Readonly<AvatarProps>) {
  const round = corner ?? radius.pill;
  return (
    <Box w={size} h={size} radius={round} center overflow="hidden">
      <Img src={src} background={gradient} radius={round} fill alt={name} />
      {src ? null : (
        <Txt
          style={{
            fontFamily: fonts.display,
            fontWeight: '700',
            fontSize: Math.round(size * 0.42),
            lineHeight: Math.round(size * 0.5),
            color: 'rgba(255, 255, 255, 0.92)',
          }}
        >
          {initialsOf(name)}
        </Txt>
      )}
    </Box>
  );
}
