// Admin "Modules" page: every module installed on this server with its packaged
// icon, capabilities, an enable toggle and its config, plus the registry Store
// (browse / install / update with dependency auto-install + checksum verify),
// install-by-upload and uninstall. Backed by GET/POST/PUT /api/admin/modules
// and /api/admin/store. The Store grid lives in module-store.tsx, the
// dependency chips in module-deps.tsx.

import { sessionToken } from '@luma/core';
import { moduleIconUrl } from '@luma/module-sdk';
import { useRef, useState } from 'react';
import { type AdminModule, adminApi } from '#web/features/admin/module-api';
import { ModuleConfigForm } from '#web/features/admin/module-config-form';
import { ModuleDeps } from '#web/features/admin/module-deps';
import {
  installFromStore,
  installSummary,
  type RegistryModule,
  type StoreCatalog,
  StoreSection,
} from '#web/features/admin/module-store';
import { Denied, useCap, usePoll } from '#web/features/admin/shell';
import { Card, Pill, Toggle } from '#web/features/admin/ui';
import { useModuleSettingsPanels, useRefreshModules } from '#web/modules/ModuleHostProvider';
import { apiBase } from '#web/shared/lib/api';

/** POST a module bundle (raw .lmod bytes) to the install endpoint. */
async function installBundle(file: File): Promise<void> {
  const token = sessionToken();
  const res = await fetch(`${apiBase()}/api/admin/store/install`, {
    method: 'POST',
    headers: token ? { Authorization: `Bearer ${token}` } : {},
    body: file,
  });
  if (!res.ok) {
    throw new Error((await res.text()) || `install failed (${res.status})`);
  }
}

export function ModulesAdminPage() {
  const canManage = useCap('settings.manage');
  const refreshModules = useRefreshModules();
  const { data, reload } = usePoll(
    ['admin', 'modules'],
    () => adminApi<AdminModule[]>('/modules'),
    30000,
  );
  // The registry catalog, enriched server-side with this server's verdict per
  // module. Undefined while loading or if the registry is unreachable; the
  // Store section just hides then.
  const { data: catalog, reload: reloadCatalog } = usePoll(
    ['admin', 'store', 'catalog'],
    () => adminApi<StoreCatalog>('/store/catalog'),
    300000,
  );
  const fileRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  if (!canManage) return <Denied />;
  const modules = data ?? [];
  const installedIds = new Set(modules.map((m) => m.id));
  // Registry entry per installed module, for the update badge/button.
  const registryById = new Map((catalog?.modules ?? []).map((m) => [m.id, m]));

  const installFromRegistry = async (id: string) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const report = await installFromStore(id);
      setNotice(installSummary(report));
      await refreshModules();
      await reloadCatalog();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (id: string, enabled: boolean) => {
    try {
      await adminApi(`/modules/${encodeURIComponent(id)}/enabled`, {
        method: 'POST',
        body: JSON.stringify({ enabled }),
      });
    } catch (e) {
      // Surface instead of an unhandled rejection; the refresh below resyncs the
      // toggle to the true server state (it reverts on failure).
      console.error('[modules] failed to toggle', id, e);
    }
    // Re-snapshot the whole module host, not just this page: refreshes the
    // ['modules'] query behind `disabledIds`, so the sidebar nav, the
    // /admin/<id> route and any contributed panels reflect the toggle live -
    // no page reload. (This also refetches the admin list, so `reload()` is
    // covered.)
    await refreshModules();
  };

  const onPick = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await installBundle(file);
      // Soft-reload: load the new module's remote + re-snapshot nav/pages, so the
      // module appears immediately with no page refresh.
      await refreshModules();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const uninstall = async (id: string) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await adminApi(`/store/${encodeURIComponent(id)}`, { method: 'DELETE' });
      await refreshModules();
      await reloadCatalog();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-6 p-5">
      <div>
        <h1 className="text-2xl font-bold text-text">Modules</h1>
        <p className="text-sm text-muted">
          Install, update, enable, disable and configure the modules on this server. Add one with no
          rebuild from the registry below (dependencies and checksums are handled for you), or
          upload a module file (.lmod) directly: its backend runs as a supervised native process and
          its page as a Module Federation remote.
        </p>
      </div>

      {(error || notice) && (
        <p className={`text-xs font-semibold ${error ? 'text-danger' : 'text-success'}`}>
          {error ?? notice}
        </p>
      )}

      <StoreSection
        catalog={catalog}
        installedIds={installedIds}
        busy={busy}
        onInstall={(id) => void installFromRegistry(id)}
        onReload={() => void reloadCatalog()}
      />

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-bold uppercase tracking-wide text-dim">Install a module</h2>
        <Card className="flex flex-col gap-3 p-4">
          <input
            ref={fileRef}
            type="file"
            accept=".lmod,.tar"
            className="hidden"
            onChange={(e) => void onPick(e.target.files?.[0])}
          />
          <div className="flex items-center gap-3">
            <button
              type="button"
              disabled={busy}
              onClick={() => fileRef.current?.click()}
              className="rounded bg-accent-soft px-4 py-2 text-sm font-semibold text-accent disabled:opacity-50"
            >
              {busy ? 'Working...' : 'Upload module (.lmod)'}
            </button>
            <p className="text-xs text-muted">
              Pack one with <code className="text-dim">bun run modules:pack</code>.
            </p>
          </div>
        </Card>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-bold uppercase tracking-wide text-dim">
          Installed ({modules.length})
        </h2>
        <div className="grid gap-3 md:grid-cols-2">
          {modules.map((m) => (
            <InstalledCard
              key={m.id}
              module={m}
              all={modules}
              registry={registryById.get(m.id)}
              busy={busy}
              onSaved={reload}
              onToggle={(v) => void toggle(m.id, v)}
              onUpdate={() => void installFromRegistry(m.id)}
              onUninstall={() => void uninstall(m.id)}
            />
          ))}
        </div>
      </section>
    </div>
  );
}

function InstalledCard({
  module: m,
  all,
  registry,
  busy,
  onSaved,
  onToggle,
  onUpdate,
  onUninstall,
}: Readonly<{
  module: AdminModule;
  all: AdminModule[];
  registry: RegistryModule | undefined;
  busy: boolean;
  onSaved: () => void;
  onToggle: (enabled: boolean) => void;
  onUpdate: () => void;
  onUninstall: () => void;
}>) {
  // An update is offered only when the registry has a newer version AND a
  // compatible artifact for this server (the backend computed both).
  const update = registry?.updateAvailable && registry.compatible ? registry : undefined;
  return (
    <Card className="p-4">
      <div className="flex items-start gap-3">
        <img
          src={moduleIconUrl(m.id, apiBase())}
          alt=""
          className="mt-0.5 h-8 w-8 shrink-0 rounded-lg"
          onError={(e) => {
            e.currentTarget.style.visibility = 'hidden';
          }}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2">
            <span className="truncate font-semibold text-text">{m.name}</span>
            <Toggle on={m.enabled} onChange={onToggle} />
          </div>
          <div className="text-[11px] text-dim">
            {m.id} · v{m.version}
            {update && (
              <span className="ml-1.5 font-semibold text-accent">v{update.version} available</span>
            )}
          </div>
          {m.description && <p className="mt-1 text-xs text-muted">{m.description}</p>}
          <div className="mt-2 flex flex-wrap gap-1.5">
            {(m.provides ?? []).map((c) => (
              <Pill key={`${c.kind}:${c.id}`} bg="rgba(255,255,255,.06)">
                {c.kind}:{c.id}
              </Pill>
            ))}
          </div>
          <ModuleDeps module={m} all={all} />
          <ModuleSettings module={m} onSaved={onSaved} />
          {(update || m.removable) && (
            <div className="mt-3 flex items-center gap-2">
              {update && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={onUpdate}
                  className="rounded bg-accent-soft px-3 py-1 text-xs font-semibold text-accent disabled:opacity-50"
                >
                  Update to v{update.version}
                </button>
              )}
              {m.removable && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={onUninstall}
                  className="rounded border border-border px-3 py-1 text-xs font-semibold text-danger disabled:opacity-50"
                >
                  Uninstall
                </button>
              )}
            </div>
          )}
        </div>
      </div>
    </Card>
  );
}

/** A module's settings block: any rich `settingsPanels` the frontend module
 *  contributes, followed by the typed form generated from its `config` schema.
 *  Renders nothing when the module exposes neither. */
function ModuleSettings({
  module,
  onSaved,
}: Readonly<{ module: AdminModule; onSaved: () => void }>) {
  const { host, panels } = useModuleSettingsPanels(module.id);
  const fields = module.config ?? [];
  if (panels.length === 0 && fields.length === 0) return null;
  return (
    <>
      {host &&
        panels.map((p) => {
          const Panel = p.component;
          return (
            <div key={p.id} className="mt-3 border-t border-border pt-3">
              <Panel host={host} />
            </div>
          );
        })}
      {fields.length > 0 && (
        <ModuleConfigForm
          moduleId={module.id}
          fields={fields}
          values={module.configValues}
          onSaved={onSaved}
        />
      )}
    </>
  );
}
