// Mount point for user-facing module pages: /m/<path> resolves to the enabled
// module route registered under that path and renders it in the main app shell.
// One splat route serves every module (the route tree is static; modules are
// resolved at runtime by the registry).

import { createFileRoute } from '@tanstack/react-router';
import { ModuleRouteOutlet } from '#web/modules/ModuleRouteOutlet';

export const Route = createFileRoute('/_app/m/$')({
  component: ModulePage,
});

function ModulePage() {
  const { _splat } = Route.useParams();
  return <ModuleRouteOutlet path={_splat ?? ''} />;
}
