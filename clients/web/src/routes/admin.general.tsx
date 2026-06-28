import { createFileRoute } from '@tanstack/react-router';
import { SettingsPage } from '#web/components/admin/settings';

export const Route = createFileRoute('/admin/general')({
  component: () => (
    <SettingsPage view="general" titleKey="admin.pageGeneral" subtitleKey="admin.pageGeneralSub" />
  ),
});
