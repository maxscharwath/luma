import type { LumaModule } from '@luma/module-sdk';
import { lazy } from 'react';
import en from '../../locales/en.json';
import fr from '../../locales/fr.json';
import manifest from '../../module.json';

// The Indexers module (frontend half). id / version / dependsOn come from the
// shared module.json (same file the backend reads). It contributes the full
// Indexers admin page into the Acquisition sidebar group; the paired
// IndexersModule ServerModule gates the /api/admin/indexers routes, so disabling
// the module removes the page and its routes together.
export const indexerModule: LumaModule = {
  id: manifest.id,
  version: manifest.version,
  dependsOn: manifest.dependsOn,
  locales: { en, fr },
  navItems: [
    {
      to: '/admin/m/indexers',
      label: 'nav.indexers',
      icon: 'antenna',
      section: 'acquisition',
      requires: 'settings.manage',
    },
  ],
  routes: [{ path: 'indexers', component: lazy(() => import('./IndexersPage')) }],
};
