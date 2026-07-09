// Poster-shaped shimmer placeholders now live in the shared skeleton kit
// (src/shared/ui/skeleton.tsx). Re-exported here so existing import sites
// (search-results, trending, trending-page) keep working unchanged.

export { PosterSkeleton, SkeletonRow } from '#web/shared/ui/skeleton';
