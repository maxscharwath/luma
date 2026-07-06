import { createFileRoute } from '@tanstack/react-router';
import { NamingPage } from '#web/features/admin/naming';

export const Route = createFileRoute('/admin/naming')({
  component: NamingPage,
});
