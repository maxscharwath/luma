// <Badge>: the small quality / status pill (4K, HDR, H.265, FR, Disponible).
// Text only, never emoji.

import type { ReactNode } from 'react';
import { Box } from '../system/Box';
import { sv } from '../system/sv';
import { colors, fonts } from '../tokens';
import { Txt } from './Text';

export type BadgeTone = '4K' | 'HDR' | 'H.265' | 'success' | 'info' | 'neutral';

/** Each tone is a tinted wash of its own hue at 16%, which is why the
 * backgrounds are literal rgba rather than a token: they are derived colours,
 * and no target can compute color-mix() (old webOS cannot, React Native cannot). */
const TONE: Record<BadgeTone, { color: string; backgroundColor: string }> = {
  '4K': { color: colors.accent, backgroundColor: colors.accentSoft },
  HDR: { color: colors.hdr, backgroundColor: 'rgba(199, 146, 234, 0.16)' },
  'H.265': { color: colors.h265, backgroundColor: 'rgba(95, 211, 196, 0.16)' },
  success: { color: colors.success, backgroundColor: 'rgba(70, 208, 141, 0.16)' },
  info: { color: colors.info, backgroundColor: 'rgba(134, 168, 255, 0.16)' },
  neutral: { color: 'rgba(244, 243, 240, 0.85)', backgroundColor: 'rgba(255, 255, 255, 0.08)' },
};

const badge = sv({
  base: { alignSelf: 'flex-start', borderRadius: 6 },
  variants: {
    size: {
      sm: { paddingVertical: 4, paddingHorizontal: 9 },
      /** The 10-foot size, as used on the hero and the rail tiles. */
      tv: { paddingVertical: 5, paddingHorizontal: 11, borderRadius: 7 },
    },
  },
  defaults: { size: 'sm' },
});

const LABEL = {
  sm: { fontFamily: fonts.ui, fontWeight: '700' as const, fontSize: 11, letterSpacing: 0.44 },
  tv: { fontFamily: fonts.ui, fontWeight: '700' as const, fontSize: 13, letterSpacing: 0.26 },
};

export interface BadgeProps {
  tone?: BadgeTone;
  size?: 'sm' | 'tv';
  children?: ReactNode;
}

export function Badge({ tone = '4K', size = 'sm', children }: Readonly<BadgeProps>) {
  const { color, backgroundColor } = TONE[tone] ?? TONE.neutral;
  return (
    <Box style={badge({ size }, { backgroundColor })}>
      <Txt style={{ ...LABEL[size], color }}>{children ?? tone}</Txt>
    </Box>
  );
}
