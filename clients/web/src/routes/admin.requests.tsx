import { createFileRoute } from '@tanstack/react-router';
import { RequestsQueuePage } from '#web/features/admin/requests-queue';

export const Route = createFileRoute('/admin/requests')({
  component: RequestsQueuePage,
});
