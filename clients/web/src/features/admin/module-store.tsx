// The Store section of the admin Modules page: what the configured registry
// offers for THIS server. The backend enriches every catalog entry with the
// artifact matching the server's build target, the installed version, an
// update flag and a compatibility verdict; installing goes through
// POST /store/install-id, which resolves missing hard dependencies from the
// same catalog and verifies each download's sha256 before unpacking.

import { Image } from '@kroma/ui';
import { IconSearch } from '@tabler/icons-react';
import { type ReactNode, useState } from 'react';
import { adminApi } from '#web/features/admin/module-api';
import { Card } from '#web/features/admin/ui';
import { InputGroup, InputGroupAddon, InputGroupInput } from '#web/shared/ui/input-group';

/** One catalog entry, enriched server-side (GET /api/admin/store/catalog). */
export interface RegistryModule {
  id: string;
  name: string;
  version: string;
  description?: string;
  library?: boolean;
  /** Packaged icon inlined as a data URI by the catalog generator, so it shows
   *  before the module is downloaded. */
  icon?: string | null;
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

/** The enriched catalog response. `error` is set (with an empty module list)
 *  when the registry could not be fetched; `registryUrl` is always present. */
export interface StoreCatalog {
  schema: number;
  serverVersion: string;
  target: string;
  registryUrl: string;
  error?: string | null;
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
    <Card className="flex items-start gap-3 p-4">
      {m.icon ? (
        <Image src={m.icon} fit="cover" className="mt-0.5 h-9 w-9 shrink-0 rounded-lg" />
      ) : (
        <div className="mt-0.5 h-9 w-9 shrink-0 rounded-lg bg-white/5" />
      )}
      <div className="min-w-0 flex-1">
        <div className="flex items-center justify-between gap-2">
          <span className="truncate font-semibold text-text">{m.name}</span>
          <button
            type="button"
            disabled={busy || !m.compatible}
            title={m.compatible ? undefined : (m.reason ?? undefined)}
            onClick={() => onInstall(m.id)}
            className="shrink-0 rounded bg-accent-soft px-3 py-1.5 text-xs font-semibold text-accent disabled:opacity-50"
          >
            Install
          </button>
        </div>
        <div className="text-[11px] text-dim">
          {m.id} · v{m.version}
          {m.size ? <> · {Math.trunc(m.size / 1024)} KB</> : null}
        </div>
        {m.description && <p className="mt-1 text-xs text-muted">{m.description}</p>}
        {!m.compatible && m.reason && (
          <p className="mt-1 text-xs font-semibold text-danger">{m.reason}</p>
        )}
      </div>
    </Card>
  );
}

/** Case-insensitive match of a catalog entry against a search query
 *  (id, name and description all count). */
function matches(m: RegistryModule, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [m.id, m.name, m.description ?? ''].some((s) => s.toLowerCase().includes(q));
}

/** Inline registry-URL editor, shown when the registry is unreachable (and as
 *  the escape hatch to point the Store at any other catalog: a GitHub release,
 *  gh-pages, a NAS...). Saves the `moduleRegistryUrl` setting then refetches. */
function RegistryUrlEditor({
  current,
  onSaved,
}: Readonly<{ current: string; onSaved: () => void }>) {
  const [url, setUrl] = useState(current);
  const [saving, setSaving] = useState(false);
  const save = async () => {
    setSaving(true);
    try {
      await adminApi('/settings', {
        method: 'PUT',
        body: JSON.stringify({ moduleRegistryUrl: url.trim() }),
      });
      onSaved();
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="flex flex-wrap items-center gap-2">
      <InputGroup className="h-9 min-w-72 flex-1">
        <InputGroupInput
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://.../modules.json"
          className="text-[12px]"
        />
      </InputGroup>
      <button
        type="button"
        disabled={saving || !url.trim()}
        onClick={() => void save()}
        className="rounded bg-accent-soft px-3 py-1.5 text-xs font-semibold text-accent disabled:opacity-50"
      >
        {saving ? 'Saving...' : 'Save & retry'}
      </button>
    </div>
  );
}

/** The registry ("Store") section: always visible so the registry state is
 *  never a mystery. Shows the catalog grid with a search box when reachable,
 *  and an explicit error card (failing URL + inline URL editor + how to
 *  publish) when not. */
export function StoreSection({
  catalog,
  installedIds,
  busy,
  onInstall,
  onReload,
}: Readonly<{
  catalog: StoreCatalog | null | undefined;
  installedIds: Set<string>;
  busy: boolean;
  onInstall: (id: string) => void;
  onReload: () => void;
}>) {
  const [query, setQuery] = useState('');
  if (!catalog) {
    return (
      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-bold uppercase tracking-wide text-dim">Registry</h2>
        <p className="text-xs text-muted">Loading the module registry...</p>
      </section>
    );
  }
  if (catalog.error) {
    return (
      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-bold uppercase tracking-wide text-dim">Registry</h2>
        <Card className="flex flex-col gap-3 p-4">
          <p className="text-sm font-semibold text-danger">Module registry unreachable</p>
          <p className="break-all text-xs text-muted">{catalog.error}</p>
          <p className="text-xs text-muted">
            The default registry is the <code className="text-dim">modules.json</code> attached to
            this project's GitHub Releases; it exists once a release is published (tag{' '}
            <code className="text-dim">vX.Y.Z</code>). You can also point the Store at any other
            catalog URL:
          </p>
          <RegistryUrlEditor current={catalog.registryUrl} onSaved={onReload} />
        </Card>
      </section>
    );
  }
  const available = catalog.modules.filter((m) => !installedIds.has(m.id));
  const shown = available.filter((m) => matches(m, query));
  let body: ReactNode;
  if (available.length === 0) {
    body = (
      <p className="text-xs text-muted">
        Every module from the registry ({catalog.modules.length}) is installed.
      </p>
    );
  } else if (shown.length === 0) {
    body = <p className="text-xs text-muted">No module matches "{query.trim()}".</p>;
  } else {
    body = (
      <div className="grid gap-3 md:grid-cols-2">
        {shown.map((m) => (
          <StoreCard key={m.id} m={m} busy={busy} onInstall={onInstall} />
        ))}
      </div>
    );
  }
  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-sm font-bold uppercase tracking-wide text-dim">
          Available in the registry ({available.length})
        </h2>
        {available.length > 0 && (
          <InputGroup className="h-9 w-64">
            <InputGroupAddon>
              <IconSearch size={15} />
            </InputGroupAddon>
            <InputGroupInput
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search modules..."
              className="text-[13px]"
            />
          </InputGroup>
        )}
      </div>
      {body}
    </section>
  );
}
