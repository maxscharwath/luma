import { createFileRoute } from '@tanstack/react-router';
import { StorePage } from '#web/features/admin/store';

export const Route = createFileRoute('/admin/store')({
  component: StorePage,
});
