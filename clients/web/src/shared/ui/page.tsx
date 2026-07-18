// The single layout dialect for EVERY page in the app (catalogue pages AND the
// admin console, via PageHeader) so titles, subtitles, gutters, and vertical
// rhythm are identical everywhere. Change a value here and it moves on all pages.

// EmptyState is a shared design-system primitive (@kroma/ui); re-exported here so
// pages keep importing it from `#web/shared/ui` alongside the page-layout tokens.
export { EmptyState } from '@kroma/ui';

/** Standard page wrapper: full-width, page gutter, vertical rhythm. Applied by
 * the catalogue pages directly and by the admin shell's <main>. */
export const PAGE_MAIN = 'min-w-0 px-(--gutter-web) pb-20 pt-9';

/** Standard page `<h1>`. */
export const PAGE_TITLE =
  'font-display text-[clamp(26px,5vw,32px)] font-bold leading-tight tracking-[-.02em]';

/** Dim one-liner under the title. */
export const PAGE_SUBTITLE = 'mt-1.5 text-[14.5px] font-medium text-dim max-sm:text-[15.5px]';
