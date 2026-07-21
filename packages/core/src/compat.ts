// Client -> server version compatibility, shared by every client.
//
// The check is CLIENT-LED and one-directional: a standalone client (TV / Android /
// desktop) ships independently of the server, so each build carries the oldest
// SERVER it works with (`minServerVersion`) and compares the server it connects to
// (its `version`, from GET /api/health) against that. A server older than the
// client needs produces a (non-blocking) warning; anything unknown is treated as
// compatible so a dev build never cries wolf.

/** Compare two dotted numeric versions ("0.1.31"). Only the leading numeric parts
 * matter; a trailing suffix (`-rc1`, git hash, …) is ignored. Returns -1 | 0 | 1
 * for a<b | a==b | a>b. */
export function compareVersions(a: string, b: string): -1 | 0 | 1 {
  const parts = (v: string): number[] =>
    v
      .split('.')
      .map((p) => Number.parseInt(p, 10))
      .map((n) => (Number.isFinite(n) ? n : 0));
  const pa = parts(a);
  const pb = parts(b);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d < 0 ? -1 : 1;
  }
  return 0;
}

/** This client build's identity, baked in at compile time. */
export interface ClientBuild {
  /** This build's version (shown to the user). */
  version: string;
  /** The oldest SERVER version this build is compatible with. */
  minServerVersion: string;
}

/** `ok` = compatible; `server-outdated` = the connected server is older than this
 * client build needs (the user should update the server). */
export type CompatVerdict = 'ok' | 'server-outdated';

/** A version is "real" (worth comparing) when it's a concrete release, not a
 * dev/unknown placeholder - so a dev build never triggers a false warning. */
function isReal(v: string | undefined | null): v is string {
  return !!v && v !== 'unknown' && v !== '0.0.0' && v !== 'dev';
}

/** Compare the connected server's version against what this client build needs. */
export function checkServerCompat(client: ClientBuild, serverVersion: string): CompatVerdict {
  if (
    isReal(serverVersion) &&
    isReal(client.minServerVersion) &&
    compareVersions(serverVersion, client.minServerVersion) < 0
  ) {
    return 'server-outdated';
  }
  return 'ok';
}
