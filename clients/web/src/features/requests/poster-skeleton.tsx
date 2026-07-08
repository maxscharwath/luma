// Poster-shaped shimmer placeholder shown while search / trending loads, so the
// grid has structure instead of a blank gap.

export function PosterSkeleton({ width = 208 }: Readonly<{ width?: number }>) {
  return (
    <div style={{ width }} className="shrink-0">
      <div className="aspect-2/3 w-full animate-pulse rounded-lg bg-white/[0.06]" />
      <div className="mt-2.5 h-3.5 w-3/4 animate-pulse rounded bg-white/[0.06]" />
      <div className="mt-1.5 h-3 w-1/3 animate-pulse rounded bg-white/[0.04]" />
    </div>
  );
}

/** A row of skeletons for a section/rail while it loads. */
export function SkeletonRow({ count = 7 }: Readonly<{ count?: number }>) {
  return (
    <div className="flex flex-wrap gap-x-4.5 gap-y-6">
      {Array.from({ length: count }, (_, i) => (
        <PosterSkeleton key={i} />
      ))}
    </div>
  );
}
