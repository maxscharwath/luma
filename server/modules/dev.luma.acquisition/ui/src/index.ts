import { defineModule } from '@luma/module-sdk';
import { lazy } from 'react';

// The Acquisition module (frontend half). Contributes the acquisition settings
// page into the Acquisition sidebar group. It is a settings-view module (the
// backend is the shared settings endpoint), so disabling it hides the nav + page.
export const acquisitionModule = defineModule({
  pages: [
    {
      path: 'acquisition',
      component: lazy(() => import('./AcquisitionPage')),
      nav: {
        label: 'nav.acquisition',
        icon: 'magnet',
        section: 'acquisition',
        requires: 'settings.manage',
      },
    },
  ],
});
