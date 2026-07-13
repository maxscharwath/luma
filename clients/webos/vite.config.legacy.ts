import { defineConfig } from 'vite';
import { tvShellLegacyConfig } from '../tv-build/shell';
import { target } from './tv.target';

// LEGACY tier (Chromium 53-94): built AFTER the modern tier; emits dist/legacy/
// and rewrites dist/index.html into the runtime engine gate. Guarded afterwards
// by `bun ../tv-build/check-legacy.ts` (see package.json).
export default defineConfig(tvShellLegacyConfig(import.meta.url, target));
