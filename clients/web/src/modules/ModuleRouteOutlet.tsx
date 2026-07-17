// Renders the module page mounted at a splat path under / (main shell) or
// /admin (admin shell). The registry resolves the path to an enabled module's
// route; a disabled or unknown module renders the not-found state, so turning a
// module off makes its pages vanish just like its nav.

import type { KromaHost } from '@kroma/module-sdk';
import { Suspense, useMemo } from 'react';
import { useModuleHostValue, useModuleRoute, useModuleT } from '#web/modules/ModuleHostProvider';

export function ModuleRouteOutlet({ path }: Readonly<{ path: string }>) {
  const host = useModuleHostValue();
  const route = useModuleRoute(path);
  // Give the page a host whose i18n resolves the module's OWN catalog first
  // (falling back to the core catalogs). `moduleT` is stable per module + locale.
  const moduleT = useModuleT(route?.moduleId ?? '');
  const scopedHost = useMemo<KromaHost | null>(
    () =>
      host
        ? {
            ...host,
            i18n: {
              t: moduleT,
              get locale() {
                return host.i18n.locale;
              },
            },
          }
        : null,
    [host, moduleT],
  );

  if (!scopedHost) return <ModuleMessage text="Loading modules..." />;
  if (!route) {
    return (
      <ModuleMessage text="This module is not installed or has been disabled." tone="strong" />
    );
  }
  const Panel = route.component;
  return (
    <Suspense fallback={<ModuleMessage text="Loading..." />}>
      <Panel host={scopedHost} />
    </Suspense>
  );
}

function ModuleMessage({ text, tone }: Readonly<{ text: string; tone?: 'strong' }>) {
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-2 p-6">
      <p className={tone === 'strong' ? 'text-sm font-semibold text-text' : 'text-sm text-muted'}>
        {text}
      </p>
    </div>
  );
}
