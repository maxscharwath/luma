import { createFileRoute } from '@tanstack/react-router';
import { SearchPage } from '#web/features/requests/search';

export const Route = createFileRoute('/_app/search')({
  component: SearchPage,
});
