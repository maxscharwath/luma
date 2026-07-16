import { createFileRoute } from '@tanstack/react-router';
import { LogsPage } from '#web/features/admin/logs';

export const Route = createFileRoute('/admin/logs')({
  component: LogsPage,
});
