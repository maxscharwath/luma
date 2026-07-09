import { createFileRoute } from '@tanstack/react-router';
import { VpnPage } from '#web/features/admin/vpn-card';

export const Route = createFileRoute('/admin/vpn')({
  component: VpnPage,
});
