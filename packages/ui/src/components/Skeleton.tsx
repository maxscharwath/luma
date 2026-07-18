// Loading-placeholder primitives shared by the app's catalogue pages and the admin
// console (via @kroma/admin-kit): the base pulsing block plus the admin table/card
// shells. Every list here is a fixed-length placeholder that never reorders, so an
// index key is correct.

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
