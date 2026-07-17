import { useT } from '@kroma/ui';
import { IconChevronLeft } from '@tabler/icons-react';
import { useNav } from '#tv/app/router';

/**
 * Pointer-first Back affordance for the 10-foot app. The remote already has a
 * dedicated Back key (wired per screen through useFocusNav → onBack), so this
 * button exists for MOUSE users (the desktop shell, an LG Magic-Remote pointer):
 * every screen that can go back now shows something clickable.
 *
 * It deliberately carries NO `data-focus`: staying out of the spatial-focus set
 * means it never steals the initial focus from a screen's primary action, which
 * keeps the tuned remote UX intact. Renders nothing at the root of the stack
 * (unless an explicit `onClick` is given), so there's never a dead Back control.
 */
export function TvBackButton({
  onClick,
  className = '',
}: Readonly<{ onClick?: () => void; className?: string }>) {
  const nav = useNav();
  const t = useT();
  if (!onClick && !nav.canGoBack) return null;
  return (
    <button
      type="button"
      onClick={onClick ?? nav.back}
      aria-label={t('common.back')}
      className={`flex size-11 flex-none cursor-pointer items-center justify-center rounded-full border border-border bg-[rgba(10,10,12,0.78)] text-text outline-none transition-transform hover:scale-[1.08] hover:text-accent focus-visible:scale-[1.08] ${className}`}
    >
      <IconChevronLeft size={22} stroke={2} />
    </button>
  );
}
