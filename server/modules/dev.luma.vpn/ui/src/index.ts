import { defineModule } from '@luma/module-sdk';
import { lazy } from 'react';

// The VPN module (frontend half). Contributes the VPN admin page into the
// Acquisition sidebar group; the paired VpnModule ServerModule owns the routes +
// the WireGuard bridge lifecycle, so disabling the module stops the bridge and
// removes the page + routes together.
export const vpnModule = defineModule({
  pages: [
    {
      path: 'vpn',
      component: lazy(() => import('./VpnPage')),
      nav: { label: 'nav.vpn', icon: 'vpn', section: 'acquisition', requires: 'settings.manage' },
    },
  ],
});
