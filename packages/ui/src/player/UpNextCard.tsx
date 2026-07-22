import { type DimensionValue, Pressable } from 'react-native';
import { gradient } from '../primitives/css';
import { Img } from '../primitives/Img';
import { Txt } from '../primitives/Text';
import { Box } from '../system/Box';
import { colors, fonts } from '../tokens';
import { FOCUS_SCALE, FOCUS_SHADOW } from './style';

/**
 * One "À suivre" tile (§10): a 16:9 thumbnail with a duration badge, then a
 * category eyebrow, a title and an optional meta line. The same card renders in
 * the parked peek and inside the open sheet (no zoom between states); focus is
 * state-driven, so the ring comes from the `focused` prop and a pointer entering
 * the card only moves focus via `onFocus` (§15).
 */
export interface UpNextItem {
  id: string;
  title: string;
  /** e.g. "S1 E4" or a year / genre line. */
  subtitle?: string;
  /** 16:9 thumbnail preferred; falls back to a subtle gradient. */
  posterUrl?: string | null;
  /** e.g. "48 min". */
  durationLabel?: string;
  /** e.g. "Épisode" or a genre. */
  categoryLabel?: string;
}

export interface UpNextCardProps {
  item: UpNextItem;
  focused: boolean;
  onActivate: () => void;
  onFocus?: () => void;
  /** Explicit width. The sheet lays three across, the peek shows one. */
  width?: DimensionValue;
}

/** Three cards across with 26px gaps, which is the sheet's layout. */
export const UP_NEXT_COLUMNS = 3;
export const UP_NEXT_GAP = 26;

/** Deterministic, subtle amber-into-charcoal placeholder when there is no still. */
function placeholderGradient(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + (id.codePointAt(i) ?? 0)) >>> 0;
  const tilt = 138 + (h % 54);
  return `linear-gradient(${tilt}deg, rgba(244,182,66,0.16) 0%, rgba(20,18,22,0.96) 64%)`;
}

const VIGNETTE = 'radial-gradient(120% 120% at 50% 25%, transparent, rgba(0,0,0,0.42))';

export function UpNextCard({
  item,
  focused,
  onActivate,
  onFocus,
  width = '100%',
}: Readonly<UpNextCardProps>) {
  return (
    <Pressable
      onPress={onActivate}
      onPointerEnter={onFocus}
      accessibilityRole="button"
      accessibilityLabel={item.title}
      style={[{ width, borderRadius: 14 }, focused ? FOCUSED : null]}
    >
      <Box aspect={16 / 9} w="100%" radius={14} overflow="hidden" bg="surface1">
        <Img src={item.posterUrl ?? null} background={placeholderGradient(item.id)} fill />
        <Box fill pointerEvents="none" style={gradient(VIGNETTE)} />
        {item.durationLabel ? (
          <Box absolute right={10} bottom={10} radius={7} bg="rgba(0, 0, 0, 0.72)" px={9} py={3}>
            <Txt style={DURATION}>{item.durationLabel}</Txt>
          </Box>
        ) : null}
      </Box>
      {item.categoryLabel ? (
        <Txt lines={1} style={CATEGORY} color="accent">
          {item.categoryLabel}
        </Txt>
      ) : null}
      <Txt style={TITLE}>{item.title}</Txt>
      {item.subtitle ? (
        <Txt style={SUBTITLE} color="rgba(244, 243, 240, 0.5)">
          {item.subtitle}
        </Txt>
      ) : null}
    </Pressable>
  );
}

const FOCUSED = { boxShadow: FOCUS_SHADOW, transform: [{ scale: FOCUS_SCALE }] };

const DURATION = {
  fontFamily: fonts.ui,
  fontSize: 12,
  fontWeight: '700' as const,
  color: '#FFFFFF',
  fontVariant: ['tabular-nums' as const],
};

const CATEGORY = {
  marginTop: 12,
  fontFamily: fonts.ui,
  fontSize: 11,
  fontWeight: '700' as const,
  letterSpacing: 0.99,
  textTransform: 'uppercase' as const,
};

const TITLE = {
  marginTop: 4,
  fontFamily: fonts.ui,
  fontSize: 17,
  lineHeight: 21,
  fontWeight: '600' as const,
  color: colors.text,
};

const SUBTITLE = { marginTop: 3, fontFamily: fonts.ui, fontSize: 14, fontWeight: '500' as const };
