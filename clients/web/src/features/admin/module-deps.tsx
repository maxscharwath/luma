// Dependency chips for the admin Modules page: a module's hard + optional
// deps and capability requirements (colored by whether each is satisfied),
// plus the reverse edges (who depends on this module).

import { depEntries } from '@kroma/module-sdk';
import type { AdminModule } from '#web/features/admin/module-api';

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

/** Modules that depend on `module`: a hard/optional `dependsOn` on its id, or a
 *  capability `requires` this module's `provides` satisfies. The reverse edges of
 *  the dependency graph, so a provider (e.g. Downloads) shows who needs it. */
function dependents(module: AdminModule, all: AdminModule[]): AdminModule[] {
  const provides = module.provides ?? [];
  return all.filter((other) => {
    if (other.id === module.id) return false;
    const deps = [...depEntries(other.dependsOn), ...depEntries(other.optionalDependsOn)];
    if (deps.some((d) => d.id === module.id)) return true;
    return (other.requires ?? []).some((r) =>
      provides.some((c) => c.kind === r.kind && (!r.id || c.id === r.id)),
    );
  });
}

/** A module's dependency status, both directions: its hard + optional deps and
 *  capability requirements (each colored by whether it is satisfied), plus the
 *  modules that in turn depend on it. */
export function ModuleDeps({ module, all }: Readonly<{ module: AdminModule; all: AdminModule[] }>) {
  const byId = new Map(all.map((m) => [m.id, m]));
  const deps = [
    ...depEntries(module.dependsOn).map((d) => ({ ...d, optional: false })),
    ...depEntries(module.optionalDependsOn).map((d) => ({ ...d, optional: true })),
  ];
  const reqs = module.requires ?? [];
  const requiredBy = dependents(module, all);
  if (deps.length === 0 && reqs.length === 0 && requiredBy.length === 0) return null;
  return (
    <div className="mt-2 flex flex-col gap-1.5">
      {(deps.length > 0 || reqs.length > 0) && (
        <div className="flex flex-col gap-1">
          <span className="text-[10px] font-bold uppercase tracking-wide text-dim">Depends on</span>
          <div className="flex flex-wrap gap-1.5">
            {deps.map((d) => {
              const state = depState(byId.get(d.id), d.optional);
              return (
                <DepChip
                  key={d.id}
                  label={d.version ? `${d.id}@${d.version}` : d.id}
                  state={state}
                />
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
      )}
      {requiredBy.length > 0 && (
        <div className="flex flex-col gap-1">
          <span className="text-[10px] font-bold uppercase tracking-wide text-dim">
            Required by
          </span>
          <div className="flex flex-wrap gap-1.5">
            {requiredBy.map((d) => (
              <DepChip key={d.id} label={d.name} state={d.enabled ? 'ok' : 'disabled'} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
