// Runtime polyfills for a shell's LEGACY tier (old TV engines, Chromium 53-94),
// imported FIRST by the shell's src/main.legacy.ts. core-js covers the JS
// built-ins the app and its deps use beyond Chromium 53 (Object.fromEntries,
// Array.prototype.at, Promise.finally, ...); the two DOM polyfills cover what
// core-js does not ship: AbortController (Chrome 66, used by discovery/health
// checks - this build also patches fetch to honour `signal`) and
// IntersectionObserver (Chrome 51, used by the growing browse grid).
// Intl.PluralRules is already guarded in @kroma/core.
import 'core-js/stable';
import 'abortcontroller-polyfill/dist/polyfill-patch-fetch';
import 'intersection-observer';
