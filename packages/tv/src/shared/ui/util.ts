// Server-URL helpers shared across the TV auth/profile screens.

import { normalizeServerUrl } from '@kroma/core';

/** Hostname of a server URL, or `null` when it can't be parsed. */
export function hostOf(url: string): string | null {
  try {
    return new URL(url).hostname;
  } catch {
    return null;
  }
}

/** Resolve a (possibly server-relative) avatar URL against its own server. */
export function artUrl(serverUrl: string, url?: string | null): string | null {
  if (!url) return null;
  if (/^https?:\/\//.test(url)) return url;
  return `${normalizeServerUrl(serverUrl)}${url.startsWith('/') ? url : `/${url}`}`;
}
