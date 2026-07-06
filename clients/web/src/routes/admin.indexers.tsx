import { createFileRoute } from '@tanstack/react-router';
import { IndexersPage } from '#web/features/admin/indexers';

export const Route = createFileRoute('/admin/indexers')({
  component: IndexersPage,
});
