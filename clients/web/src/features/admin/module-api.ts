// Shared fetch helper for the admin module endpoints (/api/admin/modules*),
// used by both the Modules page and the Store page so the bearer + base-URL
// plumbing lives in one place instead of being copied into each.

import { sessionToken } from '@luma/core';
import type { ModuleManifest } from '@luma/module-sdk';
import { apiBase } from '#web/shared/lib/api';

/** A module as `GET /api/admin/modules` returns it: the manifest plus its
 *  runtime admin state. Shared by the Modules + Store pages. */
export interface AdminModule extends ModuleManifest {
  enabled: boolean;
  /** Current value per config field key (falls back to each field's default). */
  configValues: Record<string, unknown>;
  /** Runtime-installed (WASM) modules can be uninstalled; compile-time ones can't. */
  removable: boolean;
}

export async function adminApi<T>(path: string, init?: RequestInit): Promise<T> {
  const token = sessionToken();
  const res = await fetch(`${apiBase()}/api/admin${path}`, {
    ...init,
    headers: {
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
    },
  });
  if (!res.ok) {
    // Surface the server's message (compat verdicts, dependency conflicts,
    // checksum mismatches) instead of a bare status code.
    const text = await res.text().catch(() => '');
    throw new Error(text || `${init?.method ?? 'GET'} ${path} -> ${res.status}`);
  }
  return (res.status === 204 ? undefined : await res.json()) as T;
}
