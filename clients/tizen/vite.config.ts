import { tvShellConfig } from '../tv-build/shell';
import { target } from './tv.target';

// The shared TV-shell pipeline, parameterized by ./tv.target.ts.
// `tvShellConfig` already returns a fully-typed Vite config function; export it
// straight through. Wrapping it in this shell's own `defineConfig` would pull in
// a SECOND physical copy of vite (bun peer-dedups it separately from the one
// tv-build/shell.ts types against), and TS then rejects the two mismatched
// `UserConfig` identities. One vite identity = one happy typecheck.
export default tvShellConfig(import.meta.url, target);
