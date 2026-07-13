// The LUMA module Vite plugin. It wires a module's manifest + locales into its
// `defineModule({ ... })` call by convention, so a module entry file imports
// NEITHER `module.json` NOR its locales -- the fixed folder layout is the
// contract:
//
//   <id>/module.json          <- injected as the manifest
//   <id>/locales/*.json        <- injected as the locales (import.meta.glob)
//   <id>/ui/src/index.tsx      <- the entry that calls defineModule({ pages })
//
// So a module author writes only what is unique to the module:
//
//   export const vpnModule = defineModule({ pages: [ ... ] });
//
// and the manifest (id / version / dependsOn) + translations are filled in here.

import type { Plugin } from 'vite';

// A module's frontend entry, by convention: `<id>/ui/src/index.tsx` (compiled-in
// modules) or `<id>/ui/src/module.tsx` (Module Federation remotes).
const MODULE_ENTRY = /[\\/]ui[\\/]src[\\/](?:index|module)\.tsx?$/;

// The options-only call form: `defineModule(` immediately followed by `{`. The
// explicit `defineModule(manifest, { ... })` form does not match, so it is left
// untouched (an escape hatch for tests / non-plugin builds).
const OPTIONS_ONLY_CALL = /\bdefineModule\s*\(\s*\{/;

/** Inject a module's manifest + locales into its `defineModule({ ... })` call by
 *  convention. Add it to the Vite config of anything that bundles module UIs (the
 *  web app, each MF remote). Relative paths are resolved from the entry file, so
 *  every module resolves its own `../../module.json` + `../../locales/`. */
export function lumaModule(): Plugin {
  return {
    name: 'luma-module',
    // Run before Vite's own `import.meta.glob` transform so the injected glob is
    // expanded by core.
    enforce: 'pre',
    transform(code, id) {
      const file = id.split('?', 1)[0] ?? id;
      if (!MODULE_ENTRY.test(file) || !OPTIONS_ONLY_CALL.test(code)) return null;
      const injected = `import __lumaManifest from '../../module.json';\n${code}`.replace(
        OPTIONS_ONLY_CALL,
        "defineModule({ manifest: __lumaManifest, locales: import.meta.glob('../../locales/*.json', { eager: true, import: 'default' }),",
      );
      // Sourcemap dropped for the one shifted line; these entry files are tiny.
      return { code: injected, map: null };
    },
  };
}
