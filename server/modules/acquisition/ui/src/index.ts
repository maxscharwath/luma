import type { LumaModule } from '@luma/module-sdk';
import { lazy } from 'react';
import en from '../../locales/en.json';
import fr from '../../locales/fr.json';
import manifest from '../../module.json';

// The Acquisition module (frontend half). Contributes the acquisition settings
// page into the Acquisition sidebar group. It is a settings-view module (the
// backend is the shared settings endpoint), so disabling it hides the nav + page.
export const acquisitionModule: LumaModule = {
  id: manifest.id,
  version: manifest.version,
  locales: { en, fr },
  navItems: [
    {
      to: '/admin/m/acquisition',
      label: 'nav.acquisition',
      icon: 'magnet',
      section: 'acquisition',
      requires: 'settings.manage',
    },
  ],
  routes: [{ path: 'acquisition', component: lazy(() => import('./AcquisitionPage')) }],
};
