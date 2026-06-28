import { createFileRoute } from '@tanstack/react-router';
import { SettingsPage } from '#web/components/admin/settings';

export const Route = createFileRoute('/admin/network')({
  component: () => (
    <SettingsPage view="network" titleKey="admin.pageNetwork" subtitleKey="admin.pageNetworkSub" />
  ),
});
