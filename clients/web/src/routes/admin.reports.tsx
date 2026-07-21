import { createFileRoute } from '@tanstack/react-router';
import { ReportsQueuePage } from '#web/features/admin/reports-queue';

export const Route = createFileRoute('/admin/reports')({
  component: ReportsQueuePage,
});
