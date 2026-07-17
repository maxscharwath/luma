// Server origin resolution for TV shells, now multi-server. The list of saved
// servers lives in `@kroma/core` storage (`kroma.servers`); this module adds the
// TV-specific bits: a one-time migration from the old single `kroma.serverUrl`,
// and a build-time `VITE_KROMA_SERVER` default baked in at deploy time so a fresh
// install of a single-server appliance still finds its server.

import { loadServers, migrateStorage, type SavedServer, saveServer } from '@kroma/core';

const ENV_DEFAULT = (import.meta as unknown as { env?: Record<string, string | undefined> }).env
  ?.VITE_KROMA_SERVER;

/** The saved servers on first launch. Runs the one-time storage migration, then
 * seeds the build-time default when nothing is saved (single-server appliance). */
export function initialServers(): SavedServer[] {
  migrateStorage();
  let servers = loadServers();
  if (servers.length === 0 && ENV_DEFAULT) {
    servers = saveServer({ url: ENV_DEFAULT });
  }
  return servers;
}
