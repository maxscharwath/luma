import type { ClientBuild } from '@kroma/core';

// This client build's identity, for the server-compatibility check (see
// @kroma/core `checkServerCompat`). `version` is stamped into the bundle at build
// time (Vite `define`, injected by the shell build); an un-stamped dev build reads
// "dev", which the compat check treats as always-compatible.
declare const __KROMA_VERSION__: string | undefined;

/** The oldest SERVER version this client build is compatible with. Bump this when
 * the client starts relying on a server API that older servers don't have; until
 * then every server (>= 0.1.0) is fine and no warning fires. */
const MIN_SERVER_VERSION = '0.1.0';

/** This client build's version + its server requirement. */
export const CLIENT_BUILD: ClientBuild = {
  version: typeof __KROMA_VERSION__ === 'string' && __KROMA_VERSION__ ? __KROMA_VERSION__ : 'dev',
  minServerVersion: MIN_SERVER_VERSION,
};
