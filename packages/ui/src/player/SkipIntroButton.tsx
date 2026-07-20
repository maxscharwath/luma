import { useT } from '../i18n';
import { IconForward } from './icons';
import { FOCUS_RING } from './tw';

/**
 * Skip-intro pill (§13): a bottom-right "Passer l'intro" button shown only
 * during the detected intro window. Focus is state-driven, so on focus it takes
 * the amber ring + accent fill; it rises in with `kpl-rise`. Sits above where the
 * control bar mounts so the two never overlap.
 */
export interface SkipIntroButtonProps {
  visible: boolean;
  focused: boolean;
  onSkip: () => void;
}

export function SkipIntroButton({ visible, focused, onSkip }: Readonly<SkipIntroButtonProps>) {
  const t = useT();
  if (!visible) return null;
  const focusCls = `bg-accent text-accent-ink ${FOCUS_RING}`;
  return (
    <button
      type="button"
      onClick={onSkip}
      className={`absolute bottom-[214px] right-[34px] z-30 inline-flex cursor-pointer items-center gap-2.5 rounded-[12px] border border-[rgba(255,255,255,0.22)] px-[22px] py-3.5 font-sans text-[15px] font-bold outline-none backdrop-blur-lg transition-[transform,box-shadow,background] duration-150 ease-out animate-[kpl-rise_.3s_ease] ${focused ? focusCls : 'bg-[rgba(20,20,24,0.7)] text-white'}`}
    >
      {t('player.skipIntro')}
      <IconForward size={17} />
    </button>
  );
}
