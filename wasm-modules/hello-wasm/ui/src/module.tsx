import type { LumaModule } from '@luma/module-sdk';
import { lazy } from 'react';

// The runtime-loaded module the host registers. Exposed as `./module`; the host
// `loadRemote`s it and reads the default export. A user-facing page at
// /m/hellowasm (section "library" -> main sidebar).
const module: LumaModule = {
  id: 'dev.luma.hellowasm',
  version: '0.1.0',
  navItems: [{ to: '/m/hellowasm', label: 'Hello WASM', section: 'library' }],
  routes: [{ path: 'hellowasm', component: lazy(() => import('./Panel')) }],
};

export default module;
