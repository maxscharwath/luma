import { apiErrorText, type DownloadedSub, type RemoteSub } from '@luma/core';
import { useT } from '@luma/ui';
import { useEffect, useState } from 'react';
import { lumaClient, type MovieView } from '#web/shared/lib/api';

/** Online subtitle search + one-click download, shown inside the AV drawer. Auto-
 * searches the provider for the current title on open; downloading a result caches
 * it server-side and bubbles it up so it appears (and auto-enables) in the menu. */
export function SubtitleSearch({
  item,
  onDownloaded,
}: Readonly<{ item: MovieView; onDownloaded: (s: DownloadedSub) => void }>) {
  const t = useT();
  const [results, setResults] = useState<RemoteSub[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [done, setDone] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    lumaClient()
      .searchSubtitles(item.id)
      .then((r) => !cancelled && setResults(r))
      .catch((e: unknown) => !cancelled && setError(apiErrorText(e, t('player.subSearchError'))))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [item.id, t]);

  const download = (r: RemoteSub) => {
    setBusy(r.id);
    setError(null);
    lumaClient()
      .downloadSubtitle(item.id, { provider: r.provider, remoteId: r.id, language: r.language, label: r.label })
      .then((sub) => {
        onDownloaded(sub);
        setDone((p) => new Set(p).add(r.id));
      })
      .catch((e: unknown) => setError(apiErrorText(e, t('player.subDownloadError'))))
      .finally(() => setBusy(null));
  };

  return (
    <div className="mb-6 flex flex-col gap-2">
      {error ? (
        <div className="rounded-lg bg-red-500/15 px-3 py-2 text-[13px] text-red-300">{error}</div>
      ) : null}
      {loading ? <div className="px-1 py-3 text-[13px] text-white/50">{t('player.subSearching')}</div> : null}
      {!loading && results && results.length === 0 ? (
        <div className="px-1 py-3 text-[13px] text-white/50">{t('player.noSubResults')}</div>
      ) : null}
      {results?.map((r) => {
        const isDone = done.has(r.id);
        return (
          <button
            key={`${r.provider}:${r.id}`}
            onClick={() => !isDone && busy !== r.id && download(r)}
            disabled={busy === r.id || isDone}
            className={`flex w-full items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors
              ${isDone ? 'border-accent/40 bg-accent-soft' : 'border-white/10 bg-white/4 hover:bg-white/8'}
              ${busy === r.id ? 'opacity-50' : ''}`}
          >
            <span className="min-w-9 rounded-md bg-white/8 py-1.5 text-center text-[12px] font-bold text-white/85">
              {(r.language || 'ST').toUpperCase().slice(0, 3)}
            </span>
            <span className="flex-1 truncate text-[14px] font-medium text-text">{r.label}</span>
            <span className="shrink-0 text-[11px] text-white/40">{r.downloads.toLocaleString()} ↓</span>
            <span className="shrink-0 text-[13px] font-bold text-accent">
              {busy === r.id ? '…' : isDone ? '✓' : t('player.subDownload')}
            </span>
          </button>
        );
      })}
    </div>
  );
}
