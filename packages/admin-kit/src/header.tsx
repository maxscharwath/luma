// Admin page header + the amber primary header action button. The page layout
// constants mirror the app's single page dialect (kept in sync with
// `#web/shared/ui/page`) so a module page aligns with the built-in pages.

import { useT } from '@kroma/ui';
import { IconPlus } from '@tabler/icons-react';
import type { ReactNode } from 'react';

/** Standard page `<h1>` (mirrors `#web/shared/ui/page` PAGE_TITLE). */
export const PAGE_TITLE =
  'font-display text-[clamp(26px,5vw,32px)] font-bold leading-tight tracking-[-.02em]';

/** Dim one-liner under the title (mirrors `#web/shared/ui/page` PAGE_SUBTITLE). */
export const PAGE_SUBTITLE = 'mt-1.5 text-[14.5px] font-medium text-dim max-sm:text-[15.5px]';

export function PageHeader({
  title,
  suffix,
  subtitle,
  action,
  realtime,
}: Readonly<{
  title: string;
  suffix?: string;
  subtitle?: string;
  action?: ReactNode;
  realtime?: boolean;
}>) {
  const t = useT();
  return (
    <div className="mb-2 flex flex-wrap items-center justify-between gap-6">
      <div className="min-w-0">
        <h1 className={PAGE_TITLE}>
          {title} {suffix ? <span className="font-normal text-text/40">{suffix}</span> : null}
        </h1>
        {subtitle ? <p className={PAGE_SUBTITLE}>{subtitle}</p> : null}
      </div>
      {realtime ? (
        <div className="flex shrink-0 items-center gap-2.5 rounded-full border border-border bg-[#121216] px-4 py-2">
          <span className="h-1.75 w-1.75 animate-[kroma-breathe_2s_ease-in-out_infinite] rounded-full bg-accent" />
          <span className="text-[13px] font-semibold text-text/70">
            {t('admin.realtimeActivity')}
          </span>
        </div>
      ) : null}
      {action}
    </div>
  );
}

/** The amber primary action button used in headers ("Inviter", "Ajouter", ...). */
export function HeaderAction({
  label,
  onClick,
  plus = true,
}: Readonly<{
  label: string;
  onClick?: () => void;
  plus?: boolean;
}>) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex shrink-0 items-center gap-2 rounded-md bg-accent px-4.5 py-2.75 text-[14px] font-bold text-accent-ink transition-colors hover:bg-accent-hover"
    >
      {plus ? <IconPlus size={16} stroke={2.6} /> : null}
      {label}
    </button>
  );
}
