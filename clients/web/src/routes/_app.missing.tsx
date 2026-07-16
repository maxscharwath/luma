import { createFileRoute } from '@tanstack/react-router';
import { MissingPage } from '#web/features/requests/missing';

export const Route = createFileRoute('/_app/missing')({
  component: MissingPage,
});
