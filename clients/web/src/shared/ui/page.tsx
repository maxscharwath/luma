// The single layout dialect for EVERY page in the app — catalogue pages AND the
// admin console (via PageHeader) — so titles, subtitles, gutters, and vertical
// rhythm are identical everywhere. Change a value here and it moves on all pages.

import type { ReactNode } from 'react';

/** Standard page wrapper: full-width, page gutter, vertical rhythm. Applied by
 * the catalogue pages directly and by the admin shell's <main>. */
export const PAGE_MAIN = 'min-w-0 px-(--gutter-web) pb-20 pt-9';

/** Standard page `<h1>`. */
export const PAGE_TITLE =
  'font-display text-[clamp(26px,5vw,32px)] font-bold leading-tight tracking-[-.02em]';

/** Dim one-liner under the title. */
export const PAGE_SUBTITLE = 'mt-1.5 text-[14.5px] font-medium text-dim max-sm:text-[15.5px]';

/** Centered "nothing here" block: icon, headline, optional hint and action. */
export function EmptyState({
  icon,
  title,
  hint,
  action,
}: Readonly<{ icon: ReactNode; title: string; hint?: string; action?: ReactNode }>) {
  return (
    <div className="mt-16 flex flex-col items-center text-center">
      <div className="mb-3 text-dim">{icon}</div>
      <div className="text-[15.5px] font-semibold">{title}</div>
      {hint ? (
        <p className="mt-1 max-w-100 text-[13.5px] text-dim max-sm:text-[14.5px]">{hint}</p>
      ) : null}
      {action}
    </div>
  );
}
