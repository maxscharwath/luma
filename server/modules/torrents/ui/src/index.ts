import type { LumaModule } from '@luma/module-sdk';
import { lazy } from 'react';
import en from '../../locales/en.json';
import fr from '../../locales/fr.json';
import manifest from '../../module.json';

// The torrents module (frontend half). Its id, version and dependencies come
// from the shared module.json this also feeds the backend crate, so the two
// halves cannot drift. Only the frontend-only bits (nav icon, route, panel)
// live here.
export const torrentsModule: LumaModule = {
  id: manifest.id,
  version: manifest.version,
  dependsOn: manifest.dependsOn,
  locales: { en, fr },
  navItems: [
    {
      to: '/admin/m/torrents',
      label: 'nav.title',
      icon: 'download',
      section: 'acquisition',
      requires: 'library.manage',
    },
  ],
  routes: [{ path: 'torrents', component: lazy(() => import('./TorrentsPanel')) }],
};
