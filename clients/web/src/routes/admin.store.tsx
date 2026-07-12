import { createFileRoute, redirect } from '@tanstack/react-router';

// The store (install/uninstall) merged into /admin/modules; keep this path as a
// redirect so existing links and bookmarks still land on the combined page.
export const Route = createFileRoute('/admin/store')({
  beforeLoad: () => {
    throw redirect({ to: '/admin/modules' });
  },
});
