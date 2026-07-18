// Shared cast/crew ("person") helpers used by every client. People have no stable
// id in KROMA cast/crew are embedded in each title's TMDB metadata and matched by
// name so all of this keys off a case-insensitive name comparison.

import type { Metadata } from '@kroma/client';
import type { Translate } from './i18n';

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

type CastCredit = NonNullable<Metadata['cast']>[number];
type CrewCredit = NonNullable<Metadata['crew']>[number];

/** Fold one title's cast credits for `name` into the running involvement. */
function scanCast(acc: PersonInvolvement, cast: readonly CastCredit[], name: string): void {
  for (const c of cast) {
    if (!sameName(c.name, name)) continue;
    acc.acted = true;
    if (!acc.profileUrl && c.profileUrl) acc.profileUrl = c.profileUrl;
  }
}

/** Fold one title's crew credits for `name` into the running involvement. */
function scanCrew(acc: PersonInvolvement, crew: readonly CrewCredit[], name: string): void {
  for (const c of crew) {
    if (!sameName(c.name, name)) continue;
    if (!acc.jobs.includes(c.job)) acc.jobs.push(c.job);
    if (!acc.profileUrl && c.profileUrl) acc.profileUrl = c.profileUrl;
  }
}

/** Aggregate {@link PersonInvolvement} for `name` over many titles' metadata. */
export function personInvolvement(
  metas: ReadonlyArray<Metadata | null | undefined>,
  name: string,
): PersonInvolvement {
  const acc: PersonInvolvement = { acted: false, jobs: [], profileUrl: null };
  for (const meta of metas) {
    if (!meta) continue;
    scanCast(acc, meta.cast ?? [], name);
    scanCrew(acc, meta.crew ?? [], name);
  }
  return acc;
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

/** Localized role chips for a person: "Acteur" for any cast credit, then each
 * distinct crew job (known jobs translated; anything else shown verbatim). */
export function roleLabels(t: Translate, inv: PersonInvolvement): string[] {
  const roles: string[] = [];
  if (inv.acted) roles.push(t('person.role.actor'));
  for (const job of inv.jobs) roles.push(jobLabel(t, job));
  return [...new Set(roles)];
}

/** The localized label for a single crew job (verbatim when unknown). */
export function jobLabel(t: Translate, job: string): string {
  switch (job) {
    case 'Director':
      return t('person.role.director');
    case 'Writer':
      return t('person.role.writer');
    case 'Creator':
      return t('person.role.creator');
    default:
      return job;
  }
}
