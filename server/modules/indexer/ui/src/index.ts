import type { LumaModule } from '@luma/module-sdk';
import { lazy } from 'react';
import en from '../../locales/en.json';
import fr from '../../locales/fr.json';
import manifest from '../../module.json';

// The indexer module (frontend half). id / version / dependsOn come from the
// shared module.json (same file the backend reads).
export const indexerModule: LumaModule = {
  id: manifest.id,
  version: manifest.version,
  dependsOn: manifest.dependsOn,
  locales: { en, fr },
  navItems: [
    {
      to: '/admin/m/indexer',
      label: 'nav.title',
      icon: 'antenna',
      section: 'acquisition',
      requires: 'library.manage',
    },
  ],
  routes: [{ path: 'indexer', component: lazy(() => import('./IndexerPanel')) }],
};
