// Mount point for admin module pages: /admin/m/<path> resolves to the enabled
// module route registered under that path and renders it inside the admin shell
// (AdminLayout + the admin permission gate from admin.tsx). Same registry lookup
// as the main-shell /m/$ mount; the module's nav `section` decides which prefix
// links to it.

import { createFileRoute } from '@tanstack/react-router';
import { ModuleRouteOutlet } from '#web/modules/ModuleRouteOutlet';

export const Route = createFileRoute('/admin/m/$')({
  component: AdminModulePage,
});

function AdminModulePage() {
  const { _splat } = Route.useParams();
  return <ModuleRouteOutlet path={_splat ?? ''} />;
}
