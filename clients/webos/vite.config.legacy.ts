import { tvShellLegacyConfig } from '../tv-build/shell';
import { target } from './tv.target';

// LEGACY tier (Chromium 53-94): built AFTER the modern tier; emits dist/legacy/
// and rewrites dist/index.html into the runtime engine gate. Guarded afterwards
// by `bun ../tv-build/check-legacy.ts` (see package.json).
// Export tv-build's typed config straight through, WITHOUT re-wrapping in this
// shell's own `defineConfig` (that would introduce a second, separately-deduped
// physical vite copy and TS would reject the two `UserConfig` identities).
export default tvShellLegacyConfig(import.meta.url, target);
