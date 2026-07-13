import { defineModule } from '@luma/module-sdk';
import { lazy } from 'react';
// The module's self-contained stylesheet (Tailwind + LUMA design), emitted as a
// standalone CSS the host loads when it installs this remote.
import './styles.css';

// The runtime-loaded module the host registers. Exposed as `./module`; the host
// `loadRemote`s it and reads the default export. A user-facing page at
// /hellowasm (section "library" -> main sidebar).
export default defineModule({
  pages: [
    {
      path: 'hellowasm',
      component: lazy(() => import('./Panel')),
      nav: { label: 'Hello WASM', section: 'library' },
    },
  ],
});
