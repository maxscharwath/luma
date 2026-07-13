// Admin "Modules" page: every module installed on this server with its packaged
// icon, capabilities, an enable toggle and its config, plus install-by-upload and
// uninstall. Backed by GET/POST/PUT /api/admin/modules and /api/admin/store.

import { sessionToken } from '@luma/core';
import { depEntries, moduleIconUrl } from '@luma/module-sdk';
import { useRef, useState } from 'react';
import { type AdminModule, adminApi } from '#web/features/admin/module-api';
import { ModuleConfigForm } from '#web/features/admin/module-config-form';
import { Denied, useCap, usePoll } from '#web/features/admin/shell';
import { Card, Pill, Toggle } from '#web/features/admin/ui';
import { useModuleSettingsPanels, useRefreshModules } from '#web/modules/ModuleHostProvider';
import { apiBase } from '#web/shared/lib/api';

/** POST a module bundle (raw .tar bytes) to the install endpoint. */
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
  const fileRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!canManage) return <Denied />;
  const modules = data ?? [];

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
    // /admin/m/<id> route and any contributed panels reflect the toggle live -
    // no page reload. (This also refetches the admin list, so `reload()` is
    // covered.)
    await refreshModules();
  };

  const onPick = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    setError(null);
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
    try {
      await adminApi(`/store/${encodeURIComponent(id)}`, { method: 'DELETE' });
      await refreshModules();
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
          Install, enable, disable and configure the modules on this server. Upload a module bundle
          (.tar) to add one with no rebuild: its backend loads as a sandboxed WASM plugin and its
          page as a Module Federation remote.
        </p>
      </div>

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-bold uppercase tracking-wide text-dim">Install a module</h2>
        <Card className="flex flex-col gap-3 p-4">
          <input
            ref={fileRef}
            type="file"
            accept=".tar"
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
              {busy ? 'Working...' : 'Upload bundle (.tar)'}
            </button>
            <p className="text-xs text-muted">
              Build a demo with <code className="text-dim">bun run modules:wasm</code>.
            </p>
          </div>
          {error && <p className="text-xs font-semibold text-danger">{error}</p>}
        </Card>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-bold uppercase tracking-wide text-dim">
          Installed ({modules.length})
        </h2>
        <div className="grid gap-3 md:grid-cols-2">
          {modules.map((m) => (
            <Card key={m.id} className="p-4">
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
                    <Toggle on={m.enabled} onChange={(v) => void toggle(m.id, v)} />
                  </div>
                  <div className="text-[11px] text-dim">
                    {m.id} · v{m.version}
                  </div>
                  {m.description && <p className="mt-1 text-xs text-muted">{m.description}</p>}
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    {(m.provides ?? []).map((c) => (
                      <Pill key={`${c.kind}:${c.id}`} bg="rgba(255,255,255,.06)">
                        {c.kind}:{c.id}
                      </Pill>
                    ))}
                  </div>
                  <ModuleDeps module={m} all={modules} />
                  <ModuleSettings module={m} onSaved={reload} />
                  {m.removable && (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void uninstall(m.id)}
                      className="mt-3 self-start rounded border border-border px-3 py-1 text-xs font-semibold text-danger disabled:opacity-50"
                    >
                      Uninstall
                    </button>
                  )}
                </div>
              </div>
            </Card>
          ))}
        </div>
      </section>
    </div>
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

type DepState = 'ok' | 'missing' | 'disabled' | 'optional';

/** Colour/state for a dependency chip: absent deps are `optional`/`missing`, an
 *  installed dep is `ok` when enabled and `disabled` otherwise. */
function depState(target: AdminModule | undefined, optional: boolean): DepState {
  if (!target) return optional ? 'optional' : 'missing';
  return target.enabled ? 'ok' : 'disabled';
}

function DepChip({ label, state }: Readonly<{ label: string; state: DepState }>) {
  const cls: Record<DepState, string> = {
    ok: 'text-success',
    missing: 'text-danger',
    disabled: 'text-muted',
    optional: 'text-dim',
  };
  const suffix: Record<DepState, string> = {
    ok: '',
    missing: ' (missing)',
    disabled: ' (disabled)',
    optional: ' (optional)',
  };
  return (
    <span className={`rounded bg-white/5 px-2 py-0.5 text-[11px] ${cls[state]}`}>
      {label}
      {suffix[state]}
    </span>
  );
}

/** A module's dependency status: hard + optional deps and capability
 *  requirements, each colored by whether it is satisfied in the installed set. */
function ModuleDeps({ module, all }: Readonly<{ module: AdminModule; all: AdminModule[] }>) {
  const byId = new Map(all.map((m) => [m.id, m]));
  const deps = [
    ...depEntries(module.dependsOn).map((d) => ({ ...d, optional: false })),
    ...depEntries(module.optionalDependsOn).map((d) => ({ ...d, optional: true })),
  ];
  const reqs = module.requires ?? [];
  if (deps.length === 0 && reqs.length === 0) return null;
  return (
    <div className="mt-2 flex flex-col gap-1">
      <span className="text-[10px] font-bold uppercase tracking-wide text-dim">Dependencies</span>
      <div className="flex flex-wrap gap-1.5">
        {deps.map((d) => {
          const state = depState(byId.get(d.id), d.optional);
          return (
            <DepChip key={d.id} label={d.version ? `${d.id}@${d.version}` : d.id} state={state} />
          );
        })}
        {reqs.map((r) => {
          const provided = all.some(
            (m) =>
              m.enabled &&
              (m.provides ?? []).some((c) => c.kind === r.kind && (!r.id || c.id === r.id)),
          );
          return (
            <DepChip
              key={`cap:${r.kind}:${r.id ?? ''}`}
              label={r.id ? `${r.kind}:${r.id}` : r.kind}
              state={provided ? 'ok' : 'missing'}
            />
          );
        })}
      </div>
    </div>
  );
}
