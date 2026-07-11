import type { LumaModule } from '@luma/module-sdk';
import { lazy } from 'react';
import en from '../../locales/en.json';
import fr from '../../locales/fr.json';
import manifest from '../../module.json';

// The Remote access module (frontend half). Contributes the Remote access admin
// page into the System sidebar group; the paired RemoteModule ServerModule gates
// the /api/admin/remote routes, so disabling the module removes the page and its
// routes together.
export const remoteModule: LumaModule = {
  id: manifest.id,
  version: manifest.version,
  locales: { en, fr },
  navItems: [
    {
      to: '/admin/m/remote',
      label: 'nav.remote',
      icon: 'cloud',
      section: 'system',
      requires: 'settings.manage',
    },
  ],
  routes: [{ path: 'remote', component: lazy(() => import('./RemotePage')) }],
};
