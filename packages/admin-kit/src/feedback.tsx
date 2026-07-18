// Empty-state + loading-placeholder primitives admin pages (built-in AND
// module-contributed) use as `<Suspense>` fallbacks and "nothing here" blocks.
// These are the design system's shared versions (@kroma/ui), re-exported here so
// a module page keeps importing them from the admin-kit contract.

export { CardSkeleton, EmptyState, Skeleton, TableSkeleton } from '@kroma/ui';
