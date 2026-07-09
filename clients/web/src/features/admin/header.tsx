// Admin page header + the amber primary header action button. Split out of
// `shell.tsx`, which re-exports these so call sites keep importing them from
// `#web/features/admin/shell`.

import { useT } from '@luma/ui';
import { IconPlus } from '@tabler/icons-react';
import type { ReactNode } from 'react';
import { PAGE_SUBTITLE, PAGE_TITLE } from '#web/shared/ui';

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
          <span className="h-1.75 w-1.75 animate-[luma-breathe_2s_ease-in-out_infinite] rounded-full bg-accent" />
          <span className="text-[13px] font-semibold text-text/70">
            {t('admin.realtimeActivity')}
          </span>
        </div>
      ) : null}
      {action}
    </div>
  );
}

/** The amber primary action button used in headers ("Inviter", "Ajouter", …). */
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
