import { Pressable } from 'react-native';
import { useT } from '../i18n';
import { Txt } from '../primitives/Text';
import { Box } from '../system/Box';
import { colors, fonts } from '../tokens';
import { IconForward } from './icons';
import { FOCUS_SCALE, FOCUS_SHADOW } from './style';

/**
 * Skip-intro pill (§13): a bottom-right "Passer l'intro" button shown only
 * during the detected intro window. Focus is state-driven, so on focus it takes
 * the amber ring + accent fill. Sits above where the control bar mounts, so the
 * two never overlap.
 */
export interface SkipIntroButtonProps {
  visible: boolean;
  focused: boolean;
  onSkip: () => void;
}

export function SkipIntroButton({ visible, focused, onSkip }: Readonly<SkipIntroButtonProps>) {
  const t = useT();
  if (!visible) return null;
  const ink = focused ? colors.accentInk : '#FFFFFF';
  return (
    <Box absolute bottom={214} right={34} z={30}>
      <Pressable onPress={onSkip} accessibilityRole="button" accessibilityLabel={t('player.skipIntro')}>
        <Box
          row
          align="center"
          gap={10}
          radius={12}
          borderWidth={1}
          border="rgba(255, 255, 255, 0.22)"
          px={22}
          py={14}
          bg={focused ? colors.accent : 'rgba(20, 20, 24, 0.7)'}
          style={focused ? FOCUSED : null}
        >
          <Txt style={{ fontFamily: fonts.ui, fontSize: 15, fontWeight: '700' }} color={ink}>
            {t('player.skipIntro')}
          </Txt>
          <IconForward size={17} color={ink} />
        </Box>
      </Pressable>
    </Box>
  );
}

const FOCUSED = { boxShadow: FOCUS_SHADOW, transform: [{ scale: FOCUS_SCALE }] };
