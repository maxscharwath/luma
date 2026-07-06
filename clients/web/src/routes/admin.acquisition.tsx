import { createFileRoute } from '@tanstack/react-router';
import { SettingsPage } from '#web/features/admin/settings';

export const Route = createFileRoute('/admin/acquisition')({
  component: () => (
    <SettingsPage
      view="acquisition"
      titleKey="admin.pageAcquisition"
      subtitleKey="admin.pageAcquisitionSub"
    />
  ),
});
