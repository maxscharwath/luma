import { Pressable } from 'react-native';
import { useT } from '../i18n';
import { gradient } from '../primitives/css';
import { Img } from '../primitives/Img';
import { ProgressRing } from '../primitives/ProgressRing';
import { Txt } from '../primitives/Text';
import { Box } from '../system/Box';
import { colors, fonts } from '../tokens';
import { IconPlay } from './icons';

/**
 * Minimal shape the credits card needs from the up-next item. Declared locally
 * (rather than importing `UpNextItem`) so this file never hard-depends on the
 * sheet module's build order; the orchestrator passes a compatible object.
 */
export interface CreditsCardItem {
  title: string;
  /** The "kind" line under the title (e.g. "S1 E4" or a genre). */
  subtitle?: string;
  posterUrl?: string | null;
}

export interface CreditsCardProps {
  item: CreditsCardItem;
  /** Remaining whole seconds before autoplay (e.g. 5..0). */
  secondsLeft: number;
  /** Countdown length the ring drains against (e.g. 5). */
  total: number;
  playFocused: boolean;
  cancelFocused: boolean;
  onPlay: () => void;
  onCancel: () => void;
}

const ART_FILL = 'linear-gradient(135deg, rgba(244,182,66,0.16), rgba(20,18,22,0.96))';
const VIGNETTE = 'radial-gradient(120% 120% at 50% 25%, transparent, rgba(0,0,0,0.5))';

/**
 * Credits autoplay card (§11): a bottom-right card that surfaces during the
 * closing credits with the next episode, a draining amber countdown ring around
 * the seconds-left number, and a cancel escape.
 *
 * The ring is the shared <ProgressRing> rather than the design's conic-gradient:
 * a conic gradient is CSS-only, and an SVG arc is the same picture on every
 * platform.
 */
export function CreditsCard({
  item,
  secondsLeft,
  total,
  playFocused,
  cancelFocused,
  onPlay,
  onCancel,
}: Readonly<CreditsCardProps>) {
  const t = useT();
  const progress = total > 0 ? Math.max(0, Math.min(1, secondsLeft / total)) : 0;
  return (
    <Box
      absolute
      right={40}
      bottom={56}
      z={38}
      w={392}
      radius={20}
      borderWidth={1}
      border="rgba(255, 255, 255, 0.12)"
      bg="rgba(16, 16, 20, 0.9)"
      p={20}
      style={CARD_SHADOW}
    >
      <Box h={150} mb={16} radius={14} overflow="hidden">
        <Img src={item.posterUrl ?? null} background={ART_FILL} fill />
        <Box fill pointerEvents="none" style={gradient(VIGNETTE)} />
        <Box absolute left={14} bottom={14} w={54} h={54} center>
          <Box absolute>
            <ProgressRing
              value={progress}
              size={54}
              stroke={6}
              track="rgba(255, 255, 255, 0.14)"
              fill={colors.accent}
            />
          </Box>
          <Box w={42} h={42} center radius="pill" bg="#101014">
            <Txt style={COUNTDOWN}>{String(secondsLeft)}</Txt>
          </Box>
        </Box>
      </Box>
      <Txt style={EYEBROW} color="rgba(244, 243, 240, 0.5)">
        {t('player.nextEpisode')}
      </Txt>
      <Txt lines={1} style={TITLE}>
        {item.title}
      </Txt>
      {item.subtitle ? (
        <Txt style={SUBTITLE} color="accent">
          {item.subtitle}
        </Txt>
      ) : null}
      <Box row gap={12} mt={16}>
        <Action
          label={t('player.cancel')}
          onPress={onCancel}
          background={cancelFocused ? 'rgba(255, 255, 255, 0.16)' : 'rgba(255, 255, 255, 0.08)'}
          ink={colors.text}
        />
        <Action
          label={t('player.playNow')}
          onPress={onPlay}
          background={playFocused ? colors.accentHover : colors.accent}
          ink={colors.accentInk}
          grow
          icon={<IconPlay size={17} color={colors.accentInk} />}
        />
      </Box>
    </Box>
  );
}

function Action({
  label,
  onPress,
  background,
  ink,
  grow,
  icon,
}: Readonly<{
  label: string;
  onPress: () => void;
  background: string;
  ink: string;
  grow?: boolean;
  icon?: React.ReactNode;
}>) {
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      style={{
        flex: grow ? 1 : undefined,
        flexShrink: grow ? undefined : 0,
        flexDirection: 'row',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 8,
        borderRadius: 11,
        paddingHorizontal: grow ? 0 : 18,
        paddingVertical: 12,
        backgroundColor: background,
      }}
    >
      {icon}
      <Txt style={{ fontFamily: fonts.ui, fontSize: 14, fontWeight: '700' }} color={ink}>
        {label}
      </Txt>
    </Pressable>
  );
}

const CARD_SHADOW = { boxShadow: '0 26px 64px rgba(0, 0, 0, 0.62)' };

const COUNTDOWN = {
  fontFamily: fonts.ui,
  fontSize: 19,
  fontWeight: '700' as const,
  color: '#FFFFFF',
  fontVariant: ['tabular-nums' as const],
};

const EYEBROW = {
  fontFamily: fonts.ui,
  fontSize: 11,
  fontWeight: '700' as const,
  letterSpacing: 1.76,
  textTransform: 'uppercase' as const,
};

const TITLE = {
  marginTop: 4,
  fontFamily: fonts.display,
  fontSize: 19,
  lineHeight: 23,
  fontWeight: '700' as const,
};

const SUBTITLE = { marginTop: 3, fontFamily: fonts.ui, fontSize: 13, fontWeight: '600' as const };
