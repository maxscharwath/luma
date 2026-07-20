import { tvShellConfig } from '../tv-build/shell';
import { target } from './tv.target';

// The shared TV-shell pipeline, parameterized by ./tv.target.ts. This is the
// MODERN tier; the legacy tier (vite.config.legacy.ts) runs after it in `build`.
// Export tv-build's typed config straight through, WITHOUT re-wrapping in this
// shell's own `defineConfig` (that would introduce a second, separately-deduped
// physical vite copy and TS would reject the two `UserConfig` identities).
export default tvShellConfig(import.meta.url, target);
