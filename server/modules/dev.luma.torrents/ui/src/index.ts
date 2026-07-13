import { defineModule } from '@luma/module-sdk';
import { lazy } from 'react';

// The Downloads module (frontend half). Its id, version and dependencies come
// from the shared module.json this also feeds the backend crate, so the two
// halves cannot drift. It contributes the full "Downloads" admin page (the live
// queue + download-clients section) into the Acquisition sidebar group; disabling
// the module removes the page and 404s its backend routes together.
export const torrentsModule = defineModule({
  pages: [
    {
      path: 'downloads',
      component: lazy(() => import('./DownloadsPage')),
      nav: { label: 'nav.downloads', icon: 'download', section: 'acquisition' },
    },
  ],
});
