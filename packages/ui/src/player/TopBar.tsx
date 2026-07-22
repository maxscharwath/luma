import { Pressable } from 'react-native';
import { useT } from '../i18n';
import { gradient } from '../primitives/css';
import { Txt } from '../primitives/Text';
import { Box } from '../system/Box';
import { fonts } from '../tokens';
import { IconBack } from './icons';
import { FOCUS_SCALE, FOCUS_SHADOW } from './style';

/**
 * Player top chrome (§ top chrome): a gradient bar holding the round back
 * button, the title + subtitle, and an optional warning pill on the right (a
 * transcode / unsupported-codec notice, say). Rendered over the video, so the
 * bar itself is click-through and only the back button captures the pointer.
 */
export interface TopBarProps {
  title: string;
  subtitle?: string;
  /** Pre-translated warning message, or null to hide the pill. */
  warn?: string | null;
  onBack: () => void;
  /** Whether the nav machine currently rests on the back button. */
  backFocused?: boolean;
}

const SCRIM = 'linear-gradient(180deg, rgba(0,0,0,0.65), transparent)';

export function TopBar({ title, subtitle, warn, onBack, backFocused }: Readonly<TopBarProps>) {
  const t = useT();
  return (
    <Box
      absolute
      left={0}
      right={0}
      top={0}
      row
      align="center"
      gap={18}
      px={34}
      py={26}
      pointerEvents="box-none"
      style={gradient(SCRIM)}
    >
      <Pressable onPress={onBack} accessibilityRole="button" accessibilityLabel={t('player.back')}>
        <Box
          w={42}
          h={42}
          shrink={0}
          center
          radius="pill"
          borderWidth={1}
          border="rgba(255, 255, 255, 0.14)"
          bg="rgba(255, 255, 255, 0.1)"
          style={backFocused ? FOCUSED : null}
        >
          <IconBack size={20} />
        </Box>
      </Pressable>
      <Box style={{ minWidth: 0 }}>
        <Txt lines={1} style={TITLE}>
          {title}
        </Txt>
        {subtitle ? (
          <Txt lines={1} style={SUBTITLE} color="rgba(244, 243, 240, 0.6)">
            {subtitle}
          </Txt>
        ) : null}
      </Box>
      {warn ? (
        <Box shrink={0} ml="auto" radius="pill" bg="accentSoft" px={14} py={8}>
          <Txt style={{ fontFamily: fonts.ui, fontSize: 13, fontWeight: '600' }} color="accent">
            {warn}
          </Txt>
        </Box>
      ) : null}
    </Box>
  );
}

const FOCUSED = { boxShadow: FOCUS_SHADOW, transform: [{ scale: FOCUS_SCALE }] };
const TITLE = { fontFamily: fonts.display, fontSize: 19, fontWeight: '700' as const, color: '#FFFFFF' };
const SUBTITLE = { fontFamily: fonts.ui, fontSize: 13, fontWeight: '500' as const };
