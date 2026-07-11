import type { LumaModule } from '@luma/module-sdk';
import { lazy } from 'react';
import en from '../../locales/en.json';
import fr from '../../locales/fr.json';
import manifest from '../../module.json';

// The VPN module (frontend half). Contributes the VPN admin page into the
// Acquisition sidebar group; the paired VpnModule ServerModule owns the routes +
// the WireGuard bridge lifecycle, so disabling the module stops the bridge and
// removes the page + routes together.
export const vpnModule: LumaModule = {
  id: manifest.id,
  version: manifest.version,
  locales: { en, fr },
  navItems: [
    {
      to: '/admin/m/vpn',
      label: 'nav.vpn',
      icon: 'vpn',
      section: 'acquisition',
      requires: 'settings.manage',
    },
  ],
  routes: [{ path: 'vpn', component: lazy(() => import('./VpnPage')) }],
};
