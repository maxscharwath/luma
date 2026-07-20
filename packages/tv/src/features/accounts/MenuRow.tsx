import { IconChevronRight } from '@tabler/icons-react';
import type { ReactNode } from 'react';

const MENU_ROW =
  'flex w-full items-center gap-4 rounded-[15px] border border-border bg-[rgba(255,255,255,0.03)] px-5 py-4 text-left outline-none transition-transform focus:scale-[1.02] focus:border-accent';

/** One focusable settings row (icon + label + trailing value or chevron),
 * shared by the profile menu and the signed-out device-settings screen. */
export function MenuRow({
  icon,
  label,
  onAct,
  children,
}: Readonly<{
  icon: ReactNode;
  label: string;
  onAct: () => void;
  children?: ReactNode;
}>) {
  return (
    <button data-focus="" type="button" onClick={onAct} className={MENU_ROW}>
      <span className="flex h-10.5 w-10.5 flex-none items-center justify-center rounded-xl bg-[rgba(255,255,255,0.06)] text-muted">
        {icon}
      </span>
      <span className="flex-1 font-sans text-[18px] font-bold text-text">{label}</span>
      {children ?? <IconChevronRight size={20} className="text-dim" />}
    </button>
  );
}
