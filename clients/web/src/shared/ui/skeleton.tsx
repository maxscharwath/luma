// Loading-placeholder kit. One `Skeleton` primitive (a pulsing surface block)
// plus composites shaped like the real layouts they stand in for — rails, the
// title-detail page, admin tables/cards — so a loading screen keeps the page's
// structure instead of a blank gap or a spinner. Used as `<Suspense>` fallbacks
// and route `pendingComponent`s. Every list here is a fixed-length placeholder
// that never reorders, so an index key is correct.

/** A single pulsing placeholder block. Size/shape via `className` (Tailwind). */
export function Skeleton({ className = '' }: Readonly<{ className?: string }>) {
  return <div className={`animate-pulse rounded bg-white/[0.06] ${className}`} />;
}

/** A stack of text-line placeholders; the last line is shortened like real text. */
export function SkeletonText({
  lines = 3,
  className = '',
}: Readonly<{ lines?: number; className?: string }>) {
  return (
    <div className={`space-y-2 ${className}`}>
      {Array.from({ length: lines }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length placeholder row
        <Skeleton key={i} className={`h-3.5 ${i === lines - 1 ? 'w-2/3' : 'w-full'}`} />
      ))}
    </div>
  );
}

/** Poster-shaped placeholder (2:3 art + title + subtitle), matching `Poster`
 * (same fluid `--card-w` default width). */
export function PosterSkeleton({ width }: Readonly<{ width?: number }>) {
  return (
    <div style={{ width: width ?? 'var(--card-w)' }} className="shrink-0">
      <Skeleton className="aspect-2/3 w-full rounded-lg" />
      <Skeleton className="mt-2.5 h-3.5 w-3/4" />
      <Skeleton className="mt-1.5 h-3 w-1/3 bg-white/[0.04]" />
    </div>
  );
}

/** A wrapping grid of poster skeletons (search / trending / list pages).
 * Mirrors the pages' auto-fill GRID (cards.tsx) so tiles line up. */
export function SkeletonRow({ count = 7 }: Readonly<{ count?: number }>) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(min(var(--card-w),100%),1fr))] gap-x-4.5 gap-y-6 *:w-full!">
      {Array.from({ length: count }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length placeholder grid
        <PosterSkeleton key={i} />
      ))}
    </div>
  );
}

/** One catalogue rail: a section heading bar + a horizontal run of posters. */
export function RailSkeleton({ count = 7 }: Readonly<{ count?: number }>) {
  return (
    <section>
      <Skeleton className="mb-5 mt-10 h-6 w-52" />
      <div className="flex gap-[18px] overflow-hidden py-4">
        {Array.from({ length: count }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length placeholder rail
          <PosterSkeleton key={i} />
        ))}
      </div>
    </section>
  );
}

/** The home / list route shell: a hero band followed by a few rails. */
export function PageSkeleton({ rails = 3 }: Readonly<{ rails?: number }>) {
  return (
    <main className="min-w-0 px-(--gutter-web) pb-20 pt-9">
      <Skeleton className="h-[46vh] min-h-80 w-full rounded-2xl" />
      {Array.from({ length: rails }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length placeholder rails
        <RailSkeleton key={i} />
      ))}
    </main>
  );
}

/** The title-detail route shell: backdrop hero + meta column + one rail. */
export function DetailSkeleton() {
  return (
    <main className="pb-16">
      <div className="relative h-[56vh] min-h-96 w-full overflow-hidden">
        <Skeleton className="h-full w-full rounded-none" />
      </div>
      <div className="px-(--gutter-web)">
        <Skeleton className="-mt-24 h-10 w-2/5" />
        <div className="mt-4 flex gap-3">
          <Skeleton className="h-6 w-16" />
          <Skeleton className="h-6 w-16" />
          <Skeleton className="h-6 w-24" />
        </div>
        <SkeletonText className="mt-6 max-w-2xl" lines={3} />
        <div className="mt-8 flex gap-3">
          <Skeleton className="h-12 w-36 rounded-xl" />
          <Skeleton className="h-12 w-12 rounded-xl" />
        </div>
        <RailSkeleton count={6} />
      </div>
    </main>
  );
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
