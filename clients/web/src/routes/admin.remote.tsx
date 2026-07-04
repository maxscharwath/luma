import { createFileRoute } from '@tanstack/react-router';
import { RemotePage } from '#web/features/admin/remote';

export const Route = createFileRoute('/admin/remote')({
  component: RemotePage,
});
