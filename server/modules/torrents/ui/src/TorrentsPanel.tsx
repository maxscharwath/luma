import type { ModuleComponentProps, ModuleManifest } from '@luma/module-sdk';
import { useEffect, useState } from 'react';
import ownManifest from '../../module.json';

/** Torrents admin panel. Reads the backend module manifest through the host API
 *  and lists the download-client engines the backend reports it provides -
 *  proving the frontend panel and the `luma-torrent` backend module are joined
 *  by the shared id "torrents". Default export so the module can `React.lazy`
 *  it into its own chunk. */
export default function TorrentsPanel({ host }: ModuleComponentProps) {
  const [manifest, setManifest] = useState<ModuleManifest | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    host.api
      .listModules()
      .then((mods) => {
        if (alive) setManifest(mods.find((m) => m.id === ownManifest.id) ?? null);
      })
      .catch((e: unknown) => {
        if (alive) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      alive = false;
    };
  }, [host]);

  const engines = (manifest?.provides ?? [])
    .filter((c) => c.kind === 'download-client')
    .map((c) => c.id);

  return (
    <section className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold text-text">Torrent downloads</h2>
        <p className="text-sm text-muted">{manifest?.description ?? 'Loading module...'}</p>
      </div>
      {error && <p className="text-sm text-danger">{error}</p>}
      <div>
        <div className="mb-2 text-[11px] font-bold uppercase tracking-wide text-dim">
          Download engines
        </div>
        <div className="flex flex-wrap gap-2">
          {engines.map((id) => (
            <span key={id} className="rounded-full bg-white/5 px-3 py-1 text-sm text-text">
              {id}
            </span>
          ))}
        </div>
      </div>
      <p className="text-xs text-dim">host locale: {host.i18n.locale}</p>
    </section>
  );
}
