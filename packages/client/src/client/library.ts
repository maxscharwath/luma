// Admin library management (folders / scans).

import type { AdminLibrary } from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Libraries with folders, size and item counts (requires an admin capability). */
export function adminLibraries(ctx: RequestContext): Promise<{ libraries: AdminLibrary[] }> {
  return ctx.json<{ libraries: AdminLibrary[] }>('/admin/libraries');
}

/** Add a library and trigger a rescan (requires `library.manage`). */
export function createLibrary(
  ctx: RequestContext,
  body: { name: string; kind?: string; folders: string[] },
): Promise<{ id: string }> {
  return ctx.json<{ id: string }>('/admin/libraries', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

/** Rename / change folders / toggle auto-scan for a library. */
export async function updateLibrary(
  ctx: RequestContext,
  id: string,
  patch: { name?: string; folders?: string[]; autoScan?: boolean },
): Promise<void> {
  await ctx.json<void>(`/admin/libraries/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify(patch),
  });
}

/** Remove a library (its items are dropped on the ensuing rescan). */
export async function deleteLibrary(ctx: RequestContext, id: string): Promise<void> {
  await ctx.json<void>(`/admin/libraries/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

/** Kick a full rescan (from the libraries page). */
export async function scanLibrary(ctx: RequestContext, id: string): Promise<void> {
  await ctx.json<void>(`/admin/libraries/${encodeURIComponent(id)}/scan`, { method: 'POST' });
}
