// Empty-state + loading-placeholder primitives admin pages use as `<Suspense>`
// fallbacks and "nothing here" blocks. Self-contained copies of the app's
// shared versions so the kit needs no app import.

import type { ReactNode } from 'react';

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

/** A single pulsing placeholder block. Size/shape via `className` (Tailwind). */
export function Skeleton({ className = '' }: Readonly<{ className?: string }>) {
  return <div className={`animate-pulse rounded bg-white/[0.06] ${className}`} />;
}

/** Admin list/table placeholder: a header bar + evenly spaced rows. */
export function TableSkeleton({ rows = 8 }: Readonly<{ rows?: number }>) {
  return (
    <div className="mt-4 space-y-3">
      <div className="space-y-2 rounded-xl border border-border-strong bg-surface-1 p-3">
        {Array.from({ length: rows }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length placeholder rows
          <div key={i} className="flex items-center gap-4 py-2">
            <Skeleton className="h-9 w-9 rounded-lg" />
            <Skeleton className="h-4 flex-1" />
            <Skeleton className="h-4 w-24" />
            <Skeleton className="h-8 w-20 rounded-lg" />
          </div>
        ))}
      </div>
    </div>
  );
}

/** Admin settings/card placeholder: a titled panel with a few field rows. */
export function CardSkeleton({ fields = 4 }: Readonly<{ fields?: number }>) {
  return (
    <div className="space-y-4 rounded-xl border border-border-strong bg-surface-1 p-6">
      <Skeleton className="h-6 w-40" />
      {Array.from({ length: fields }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length placeholder fields
        <div key={i} className="space-y-2">
          <Skeleton className="h-3.5 w-28 bg-white/[0.04]" />
          <Skeleton className="h-10 w-full rounded-lg" />
        </div>
      ))}
    </div>
  );
}
