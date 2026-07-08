// Shared cast/crew ("person") helpers used by every client. People have no stable
// id in LUMA cast/crew are embedded in each title's TMDB metadata and matched by
// name so all of this keys off a case-insensitive name comparison.

import type { Metadata } from '@luma/client';

/** Case-insensitive, trimmed name equality (TMDB credit names). */
function sameName(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

/** Does `meta` credit `name` in its cast OR key crew? (case-insensitive) */
export function creditsPerson(meta: Metadata | null | undefined, name: string): boolean {
  if (!meta || !name.trim()) return false;
  return (
    (meta.cast ?? []).some((c) => sameName(c.name, name)) ||
    (meta.crew ?? []).some((c) => sameName(c.name, name))
  );
}

/** One person's involvement aggregated across a set of titles' metadata: whether
 * they appear in any cast, the distinct crew jobs they held (e.g. `Director`), and
 * the best profile photo found among the matching credits. */
export interface PersonInvolvement {
  /** True when the person appears in at least one title's cast. */
  acted: boolean;
  /** Distinct crew jobs (TMDB strings: `Director`, `Writer`, `Creator`, …), in
   * first-seen order. */
  jobs: string[];
  /** Profile photo from the first matching credit that carries one, else null. */
  profileUrl: string | null;
}

/** Aggregate {@link PersonInvolvement} for `name` over many titles' metadata. */
export function personInvolvement(
  metas: ReadonlyArray<Metadata | null | undefined>,
  name: string,
): PersonInvolvement {
  let acted = false;
  let profileUrl: string | null = null;
  const jobs: string[] = [];
  for (const meta of metas) {
    if (!meta) continue;
    for (const c of meta.cast ?? []) {
      if (!sameName(c.name, name)) continue;
      acted = true;
      if (!profileUrl && c.profileUrl) profileUrl = c.profileUrl;
    }
    for (const c of meta.crew ?? []) {
      if (!sameName(c.name, name)) continue;
      if (!jobs.includes(c.job)) jobs.push(c.job);
      if (!profileUrl && c.profileUrl) profileUrl = c.profileUrl;
    }
  }
  return { acted, jobs, profileUrl };
}

/** Recover a person's display name with its original casing from titles' metadata
 * (a URL path or stored lookup may differ in case). Falls back to `name`. */
export function personDisplayName(
  metas: ReadonlyArray<Metadata | null | undefined>,
  name: string,
): string {
  for (const meta of metas) {
    if (!meta) continue;
    for (const c of meta.cast ?? []) if (sameName(c.name, name)) return c.name;
    for (const c of meta.crew ?? []) if (sameName(c.name, name)) return c.name;
  }
  return name;
}
