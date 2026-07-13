import { defineModule } from '@luma/module-sdk';
import { lazy } from 'react';

// The Remote access module (frontend half). Contributes the Remote access admin
// page into the System sidebar group; the paired RemoteModule ServerModule gates
// the /api/admin/remote routes, so disabling the module removes the page and its
// routes together.
export const remoteModule = defineModule({
  pages: [
    {
      path: 'remote',
      component: lazy(() => import('./RemotePage')),
      nav: { label: 'nav.remote', icon: 'cloud', section: 'system', requires: 'settings.manage' },
    },
  ],
});
