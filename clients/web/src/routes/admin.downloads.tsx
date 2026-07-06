import { createFileRoute } from '@tanstack/react-router';
import { DownloadsPage } from '#web/features/admin/downloads';

export const Route = createFileRoute('/admin/downloads')({
  component: DownloadsPage,
});
