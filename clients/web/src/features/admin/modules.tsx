// Admin "Modules" page: every module installed on this server with its packaged
// icon, capabilities, an enable toggle and its config, backed by
// GET/POST/PUT /api/admin/modules.

import { depEntries, moduleIconUrl } from '@luma/module-sdk';
import { adminApi, type AdminModule } from '#web/features/admin/module-api';
import { ModuleConfigForm } from '#web/features/admin/module-config-form';
import { Denied, useCap, usePoll } from '#web/features/admin/shell';
import { Card, Pill, Toggle } from '#web/features/admin/ui';
import { useModuleSettingsPanels, useRefreshModules } from '#web/modules/ModuleHostProvider';
import { apiBase } from '#web/shared/lib/api';

export function ModulesAdminPage() {
  const canManage = useCap('settings.manage');
  const refreshModules = useRefreshModules();
  const { data, reload } = usePoll(
    ['admin', 'modules'],
    () => adminApi<AdminModule[]>('/modules'),
    30000,
  );
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

  return (
    <div className="flex flex-col gap-5 p-5">
      <div>
        <h1 className="text-2xl font-bold text-text">Modules</h1>
        <p className="text-sm text-muted">
          Enable, disable and configure the modules installed on this server.
        </p>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        {modules.map((m) => (
          <Card key={m.id} className="p-4">
            <div className="flex items-start gap-3">
              <img
                src={moduleIconUrl(m.id, apiBase())}
                alt=""
                className="mt-0.5 h-8 w-8 shrink-0"
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
              </div>
            </div>
          </Card>
        ))}
      </div>
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
          const target = byId.get(d.id);
          const state: DepState = !target
            ? d.optional
              ? 'optional'
              : 'missing'
            : target.enabled
              ? 'ok'
              : 'disabled';
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
