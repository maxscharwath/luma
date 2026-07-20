// Recent search queries, persisted per device (localStorage, like the other
// kroma:* device prefs, through the same never-throwing accessors). Most-recent-
// first, deduped case-insensitively, capped.

import { readDeviceValue, writeDeviceValue } from '#tv/app/devicePref';

const KEY = 'kroma:recent-searches';
const MAX = 8;

/** The saved recent searches, most recent first (empty when none / unavailable). */
export function getRecentSearches(): string[] {
  const raw = readDeviceValue(KEY);
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return parsed.filter((x): x is string => typeof x === 'string' && x.length > 0).slice(0, MAX);
    }
  } catch {
    /* corrupt JSON */
  }
  return [];
}

/** Record a query at the head of the list (deduped case-insensitively, capped)
 * and return the updated list. Blank queries leave the list untouched. */
export function addRecentSearch(query: string): string[] {
  const q = query.trim();
  const list = getRecentSearches();
  if (!q) return list;
  const next = [q, ...list.filter((old) => old.toLowerCase() !== q.toLowerCase())].slice(0, MAX);
  writeDeviceValue(KEY, JSON.stringify(next));
  return next;
}
