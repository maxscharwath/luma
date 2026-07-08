// Branded id types nominal `string`s so a `UserId` can never be passed where an
// `ItemId` is expected (or a raw string where an id is). Each brand is a zod
// schema whose `z.infer` gives the branded TS type. Turn a raw string into a
// brand with `UserId.of(s)` (a real zod parse the single, validated boundary),
// or `UserId.parse(s)` directly — `.of` is just the ergonomic alias.

import { z } from 'zod';

/** A branded-string id: a zod schema with an `.of(s)` helper that parses a raw
 * string into the brand (real validation, never a bare `as` assertion). The
 * paired `export type X = ReturnType<typeof X.of>` gives the branded TS type. */
function brandedId<const B extends string>(_brand: B) {
  const schema = z.string().brand<B>();
  // `parse` does the real validation; the assertion only re-applies the brand
  // the compiler can't see through the generic `B`. Callers get a branded value.
  const of = (s: string) => schema.parse(s) as z.infer<typeof schema>;
  return Object.assign(schema, { of });
}

export const UserId = brandedId('UserId');
export type UserId = ReturnType<typeof UserId.of>;

export const ItemId = brandedId('ItemId');
export type ItemId = ReturnType<typeof ItemId.of>;

export const ShowId = brandedId('ShowId');
export type ShowId = ReturnType<typeof ShowId.of>;

export const RequestId = brandedId('RequestId');
export type RequestId = ReturnType<typeof RequestId.of>;

export const IndexerId = brandedId('IndexerId');
export type IndexerId = ReturnType<typeof IndexerId.of>;

export const DownloadClientId = brandedId('DownloadClientId');
export type DownloadClientId = ReturnType<typeof DownloadClientId.of>;

export const JobRunId = brandedId('JobRunId');
export type JobRunId = ReturnType<typeof JobRunId.of>;

export const LibraryId = brandedId('LibraryId');
export type LibraryId = ReturnType<typeof LibraryId.of>;
