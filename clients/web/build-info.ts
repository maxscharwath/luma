import { execSync } from 'node:child_process';
import type { Plugin } from 'vite';
import pkg from './package.json' with { type: 'json' };

/** Build metadata exposed to the app through the virtual module `virtual:build-info`
 * (resolved by {@link buildInfoPlugin}). Collected once at build time / dev-server
 * start; the git fields degrade to `'unknown'` when building outside a checkout
 * (e.g. from a source tarball). */
export interface BuildInfo {
  version: string;
  commit: string;
  commitFull: string;
  branch: string;
  dirty: boolean;
  buildDate: string;
}

const git = (cmd: string): string | null => {
  try {
    return execSync(`git ${cmd}`, { stdio: ['ignore', 'pipe', 'ignore'] })
      .toString()
      .trim();
  } catch {
    return null;
  }
};

const buildInfo: BuildInfo = {
  version: pkg.version,
  commit: git('rev-parse --short HEAD') ?? 'unknown',
  commitFull: git('rev-parse HEAD') ?? 'unknown',
  branch: git('rev-parse --abbrev-ref HEAD') ?? 'unknown',
  dirty: Boolean(git('status --porcelain')),
  buildDate: new Date().toISOString(),
};

/** Serves `virtual:build-info` (a "fake" module with no on-disk file) so any
 * component can `import buildInfo from 'virtual:build-info'` — nothing ships to
 * prod but the resolved constants, matching the static-SPA model (no Node). */
export function buildInfoPlugin(): Plugin {
  const virtualId = 'virtual:build-info';
  const resolvedId = `\0${virtualId}`;
  const json = JSON.stringify(buildInfo);
  // Default export + named exports so both import styles work.
  const code = `export default ${json};\nexport const { version, commit, commitFull, branch, dirty, buildDate } = ${json};\n`;
  return {
    name: 'kroma-build-info',
    resolveId: (source) => (source === virtualId ? resolvedId : null),
    load: (id) => (id === resolvedId ? code : null),
  };
}
