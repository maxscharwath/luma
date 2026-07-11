// Admin "Store": install a module into the running server by uploading a bundle
// (a .tar of module.json + optional module.wasm + fe/ + icon), and manage the
// installed set. Install/uninstall hit /api/admin/store; the module then appears
// in /api/admin/modules like any other. A remote registry catalog is future work.

import { sessionToken } from '@luma/core';
import { moduleIconUrl } from '@luma/module-sdk';
import { useRef, useState } from 'react';
import { adminApi, type AdminModule } from '#web/features/admin/module-api';
import { Denied, useCap, usePoll } from '#web/features/admin/shell';
import { Card } from '#web/features/admin/ui';
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

export function StorePage() {
  const canManage = useCap('settings.manage');
  const { data } = usePoll(['admin', 'modules'], () => adminApi<AdminModule[]>('/modules'), 30000);
  const fileRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!canManage) return <Denied />;

  const installed = data ?? [];

  const onPick = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    setError(null);
    try {
      await installBundle(file);
      // A reload cleanly re-discovers modules + loads the new frontend remote and
      // re-snapshots the registry nav/pages.
      window.location.reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };

  const uninstall = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await adminApi(`/store/${encodeURIComponent(id)}`, { method: 'DELETE' });
      window.location.reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-6 p-5">
      <div>
        <h1 className="text-2xl font-bold text-text">Module store</h1>
        <p className="text-sm text-muted">
          Install a module into the running server, no rebuild. Upload a module bundle (.tar): its
          backend loads as a sandboxed WASM plugin and its page as a Module Federation remote.
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
              {busy ? 'Installing...' : 'Upload bundle (.tar)'}
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
          Installed ({installed.length})
        </h2>
        <div className="grid gap-3 md:grid-cols-3">
          {installed.map((m) => (
            <Card key={m.id} className="flex flex-col gap-2 p-4">
              <div className="flex items-center gap-3">
                <img
                  src={moduleIconUrl(m.id, apiBase())}
                  alt=""
                  className="h-9 w-9 shrink-0"
                  onError={(e) => {
                    e.currentTarget.style.visibility = 'hidden';
                  }}
                />
                <div className="min-w-0">
                  <div className="truncate font-semibold text-text">{m.name}</div>
                  <div className="text-[11px] text-dim">v{m.version}</div>
                </div>
                <span className="ml-auto shrink-0 rounded-full bg-accent-soft px-2 py-0.5 text-[10px] font-bold text-accent">
                  {m.enabled ? 'installed' : 'disabled'}
                </span>
              </div>
              {m.description && <p className="text-xs text-muted">{m.description}</p>}
              {m.removable && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void uninstall(m.id)}
                  className="mt-1 self-start rounded border border-border px-3 py-1 text-xs font-semibold text-danger disabled:opacity-50"
                >
                  Uninstall
                </button>
              )}
            </Card>
          ))}
        </div>
      </section>
    </div>
  );
}
