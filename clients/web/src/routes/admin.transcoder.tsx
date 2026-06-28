import { createFileRoute } from '@tanstack/react-router';
import { SettingsPage } from '#web/components/admin/settings';

export const Route = createFileRoute('/admin/transcoder')({
  component: () => (
    <SettingsPage
      view="transcoder"
      titleKey="admin.pageTranscoder"
      subtitleKey="admin.pageTranscoderSub"
    />
  ),
});
