// The Store section of the admin Modules page: what the configured registry
// offers for THIS server. The backend enriches every catalog entry with the
// artifact matching the server's build target, the installed version, an
// update flag and a compatibility verdict; installing goes through
// POST /store/install-id, which resolves missing hard dependencies from the
// same catalog and verifies each download's sha256 before unpacking.

import { adminApi } from '#web/features/admin/module-api';
import { Card } from '#web/features/admin/ui';

/** One catalog entry, enriched server-side (GET /api/admin/store/catalog). */
export interface RegistryModule {
  id: string;
  name: string;
  version: string;
  description?: string;
  library?: boolean;
  minServer?: string | null;
  /** Download URL of the artifact matching the server's platform, if any. */
  url?: string | null;
  size?: number | null;
  sha256?: string | null;
  installedVersion?: string | null;
  updateAvailable?: boolean;
  compatible: boolean;
  /** Human-readable blocker when not compatible. */
  reason?: string | null;
}

/** The enriched catalog response. */
export interface StoreCatalog {
  schema: number;
  serverVersion: string;
  target: string;
  modules: RegistryModule[];
}

/** What POST /store/install-id reports back: everything actually installed,
 *  auto-installed dependencies included, in install order. */
export interface InstallReport {
  requested: string;
  installed: { id: string; name: string; version: string }[];
}

/** Install/update a module (and its missing deps) from the registry. */
export function installFromStore(id: string): Promise<InstallReport> {
  return adminApi<InstallReport>('/store/install-id', {
    method: 'POST',
    body: JSON.stringify({ id }),
  });
}

/** Human summary of an install report: "Installed Acquisition 0.1.0 (+ 2
 *  dependencies: Downloads 0.1.0, Indexers 0.1.0)". */
export function installSummary(report: InstallReport): string {
  const requested = report.installed.find((m) => m.id === report.requested);
  const deps = report.installed.filter((m) => m.id !== report.requested);
  const head = requested ? `Installed ${requested.name} ${requested.version}` : 'Installed';
  if (deps.length === 0) return head;
  const list = deps.map((d) => `${d.name} ${d.version}`).join(', ');
  return `${head} (+ ${deps.length} ${deps.length === 1 ? 'dependency' : 'dependencies'}: ${list})`;
}

function StoreCard({
  m,
  busy,
  onInstall,
}: Readonly<{ m: RegistryModule; busy: boolean; onInstall: (id: string) => void }>) {
  return (
    <Card className="flex items-start justify-between gap-3 p-4">
      <div className="min-w-0">
        <div className="font-semibold text-text">{m.name}</div>
        <div className="text-[11px] text-dim">
          {m.id} · v{m.version}
          {m.size ? <> · {(m.size / 1024) | 0} KB</> : null}
        </div>
        {m.description && <p className="mt-1 text-xs text-muted">{m.description}</p>}
        {!m.compatible && m.reason && (
          <p className="mt-1 text-xs font-semibold text-danger">{m.reason}</p>
        )}
      </div>
      <button
        type="button"
        disabled={busy || !m.compatible}
        title={m.compatible ? undefined : (m.reason ?? undefined)}
        onClick={() => onInstall(m.id)}
        className="shrink-0 rounded bg-accent-soft px-3 py-1.5 text-xs font-semibold text-accent disabled:opacity-50"
      >
        Install
      </button>
    </Card>
  );
}

/** The "Available in the registry" grid: catalog modules not installed here.
 *  Renders nothing while the catalog is loading/unreachable or fully installed. */
export function StoreSection({
  catalog,
  installedIds,
  busy,
  onInstall,
}: Readonly<{
  catalog: StoreCatalog | null | undefined;
  installedIds: Set<string>;
  busy: boolean;
  onInstall: (id: string) => void;
}>) {
  const available = (catalog?.modules ?? []).filter((m) => !installedIds.has(m.id));
  if (available.length === 0) return null;
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-bold uppercase tracking-wide text-dim">
        Available in the registry ({available.length})
      </h2>
      <div className="grid gap-3 md:grid-cols-2">
        {available.map((m) => (
          <StoreCard key={m.id} m={m} busy={busy} onInstall={onInstall} />
        ))}
      </div>
    </section>
  );
}
