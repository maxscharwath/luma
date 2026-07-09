// Manual grab modal: search indexers or paste a magnet, ANALYZE the torrent's
// real file list (Sonarr/Radarr-style), pick which episodes/files to download,
// and set the target so import lands them in the right library. The detected
// entity pre-fills the form; the admin can override when detection is unsure.

import {
  apiErrorText,
  type ManualReleaseView,
  type TorrentAnalysis,
  type TorrentFileView,
} from '@luma/core';
import { useT } from '@luma/ui';
import { IconDownload, IconLoader2, IconSearch, IconWand } from '@tabler/icons-react';
import { useState } from 'react';
import { useAsyncAction } from '#web/features/admin/shell';
import { Field, Modal, ModalActions, SegmentedControl, TextInput } from '#web/features/admin/ui';
import { formatBytes } from '#web/shared/lib/adminFormat';
import { useAuth } from '#web/shared/lib/auth';

type Kind = 'movie' | 'episode' | 'season';

const KIND_COLOR: Record<string, string> = {
  movie: '#F4B642',
  episode: '#86A8FF',
  season: '#C792EA',
  series: '#C792EA',
  unknown: 'rgba(244,243,240,.55)',
};

export function ManualGrabModal({
  onClose,
  onAdded,
}: Readonly<{ onClose: () => void; onAdded: () => void }>) {
  const t = useT();
  const { client } = useAuth();
  const { busy, error, run } = useAsyncAction();

  // Search sub-panel
  const [query, setQuery] = useState('');
  const [searching, setSearching] = useState(false);
  const [results, setResults] = useState<ManualReleaseView[] | null>(null);
  const [searchErr, setSearchErr] = useState<string | null>(null);

  // Analysis
  const [analyzing, setAnalyzing] = useState(false);
  const [analysis, setAnalysis] = useState<TorrentAnalysis | null>(null);
  const [analyzeErr, setAnalyzeErr] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());

  // Target form
  const [magnet, setMagnet] = useState('');
  const [detailsUrl, setDetailsUrl] = useState<string | null>(null);
  const [kind, setKind] = useState<Kind>('movie');
  const [title, setTitle] = useState('');
  const [year, setYear] = useState('');
  const [season, setSeason] = useState('');
  const [episode, setEpisode] = useState('');

  const resetAnalysis = () => {
    setAnalysis(null);
    setAnalyzeErr(null);
    setSelected(new Set());
  };

  const doSearch = () => {
    const q = query.trim();
    if (!q) return;
    setSearching(true);
    setSearchErr(null);
    client
      .manualSearch(q)
      .then((v) => {
        setResults(v.releases);
        if (v.indexerErrors.length) setSearchErr(v.indexerErrors.join(' · '));
      })
      .catch((e) => setSearchErr(apiErrorText(e, t('manual.searchFailed'))))
      .finally(() => setSearching(false));
  };

  const pick = (r: ManualReleaseView) => {
    setMagnet(r.downloadUrl ?? '');
    setDetailsUrl(r.detailsUrl ?? null);
    setTitle(r.parsedTitle || title);
    setYear(r.year ? String(r.year) : '');
    resetAnalysis();
  };

  const analyze = () => {
    const m = magnet.trim();
    if (!m) return;
    setAnalyzing(true);
    setAnalyzeErr(null);
    client
      .analyzeTorrent(m)
      .then((a) => {
        setAnalysis(a);
        // Default selection = all video files.
        setSelected(new Set(a.files.filter((f) => f.isVideo).map((f) => f.index)));
        applyDetection(a);
      })
      .catch((e) => setAnalyzeErr(apiErrorText(e, t('manual.analyzeFailed'))))
      .finally(() => setAnalyzing(false));
  };

  // Pre-fill the target from the detected content (admin can still override).
  const applyDetection = (a: TorrentAnalysis) => {
    if (a.kind === 'movie') {
      setKind('movie');
    } else if (a.kind === 'episode') {
      setKind('episode');
      const ep = a.files.find((f) => f.episode != null);
      if (ep) {
        setSeason(ep.season != null ? String(ep.season) : '');
        setEpisode(String(ep.episode));
      }
    } else {
      // season / series: import per-file by parsed S/E.
      setKind('season');
      const first = a.files.find((f) => f.season != null);
      if (first?.season != null && a.seasons.length === 1) setSeason(String(first.season));
    }
  };

  const toggleFile = (i: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  };

  const videoFiles = analysis?.files.filter((f) => f.isVideo) ?? [];
  const allVideoSelected = videoFiles.length > 0 && videoFiles.every((f) => selected.has(f.index));

  const add = () =>
    run(
      async () => {
        // Only send onlyFiles when the admin narrowed the selection.
        const totalVideos = videoFiles.length;
        const onlyFiles =
          analysis && selected.size > 0 && selected.size < totalVideos
            ? [...selected].sort((a, b) => a - b)
            : null;
        await client.manualAdd({
          magnetOrUrl: magnet.trim(),
          kind,
          title: title.trim() || null,
          year: year ? Number.parseInt(year, 10) : null,
          season: kind !== 'movie' && season ? Number.parseInt(season, 10) : null,
          episode: kind === 'episode' && episode ? Number.parseInt(episode, 10) : null,
          tmdbId: null,
          onlyFiles,
          detailsUrl,
        });
        onAdded();
        onClose();
      },
      (e) => apiErrorText(e, t('manual.addFailed')),
    );

  const canAdd = magnet.trim().length > 0 && title.trim().length > 0;

  return (
    <Modal title={t('manual.title')} onClose={onClose}>
      {/* search sub-panel */}
      <div className="mb-4">
        <div className="flex gap-2">
          <div className="flex h-11 flex-1 items-center rounded-[9px] border border-border-strong bg-[#0F0F13] px-3">
            <IconSearch size={16} className="shrink-0 text-dim" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && doSearch()}
              placeholder={t('manual.searchPlaceholder')}
              className="min-w-0 flex-1 bg-transparent px-2.5 text-[13.5px] font-semibold text-text outline-none placeholder:text-dim"
            />
          </div>
          <button
            type="button"
            onClick={doSearch}
            disabled={searching || !query.trim()}
            className="inline-flex items-center gap-1.5 rounded-[9px] bg-accent px-4 text-[13px] font-bold text-accent-ink hover:bg-accent-hover disabled:opacity-50"
          >
            {searching ? <IconLoader2 size={14} stroke={2.4} className="animate-spin" /> : null}
            {t('manual.search')}
          </button>
        </div>
        {searchErr ? (
          <p className="mt-1.5 text-[12px] font-semibold text-[#F4B642]">{searchErr}</p>
        ) : null}
        {results ? (
          <div className="mt-2 max-h-44 overflow-y-auto rounded-xl border border-white/[0.07] bg-[#0F0F13]">
            {results.length === 0 ? (
              <div className="px-3 py-4 text-center text-[12.5px] font-medium text-dim">
                {t('manual.noResults')}
              </div>
            ) : (
              results.map((r) => (
                <ResultRow key={`${r.indexerName}-${r.guid}`} r={r} onPick={() => pick(r)} />
              ))
            )}
          </div>
        ) : null}
      </div>

      {/* magnet + analyze */}
      <Field label={t('manual.magnet')} hint={t('manual.magnetHint')}>
        <div className="flex gap-2">
          <TextInput
            value={magnet}
            onChange={(v) => {
              setMagnet(v);
              setDetailsUrl(null);
              resetAnalysis();
            }}
            placeholder="magnet:?xt=urn:btih:..."
            className="w-full min-w-0"
          />
          <button
            type="button"
            onClick={analyze}
            disabled={analyzing || !magnet.trim()}
            className="inline-flex shrink-0 items-center gap-1.5 rounded-[9px] border border-white/12 bg-[#1A1A20] px-3.5 text-[13px] font-semibold text-white/80 hover:bg-[#222229] disabled:opacity-50"
          >
            {analyzing ? (
              <IconLoader2 size={14} stroke={2.4} className="animate-spin" />
            ) : (
              <IconWand size={14} stroke={2} />
            )}
            {t('manual.analyze')}
          </button>
        </div>
      </Field>
      {analyzeErr ? (
        <p className="-mt-2 mb-3 text-[12px] font-semibold text-[#EF8091]">{analyzeErr}</p>
      ) : null}

      {/* analysis result: detected kind + file selection */}
      {analysis ? (
        <div className="mb-4 rounded-xl border border-white/[0.07] bg-[#0F0F13] p-3">
          <div className="mb-2 flex items-center justify-between">
            <span
              className="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11.5px] font-bold uppercase tracking-[.06em]"
              style={{
                color: KIND_COLOR[analysis.kind],
                background: `${KIND_COLOR[analysis.kind]}1f`,
              }}
            >
              {t(`manual.detected.${analysis.kind}` as Parameters<typeof t>[0])}
              {analysis.seasons.length > 0
                ? ` · ${analysis.seasons.map((s) => `S${s}`).join(' ')}`
                : ''}
            </span>
            {videoFiles.length > 1 ? (
              <button
                type="button"
                onClick={() =>
                  setSelected(
                    allVideoSelected ? new Set() : new Set(videoFiles.map((f) => f.index)),
                  )
                }
                className="text-[12px] font-semibold text-accent hover:underline"
              >
                {allVideoSelected ? t('manual.selectNone') : t('manual.selectAll')}
              </button>
            ) : null}
          </div>
          <div className="max-h-52 overflow-y-auto">
            {analysis.files.map((f) => (
              <FileRow
                key={f.index}
                f={f}
                checked={selected.has(f.index)}
                onToggle={() => toggleFile(f.index)}
              />
            ))}
          </div>
          {videoFiles.length > 1 ? (
            <div className="mt-2 text-[11.5px] font-medium text-dim">
              {t('manual.selectedCount', {
                n: String(selected.size),
                total: String(videoFiles.length),
              })}
            </div>
          ) : null}
        </div>
      ) : null}

      {/* target form */}
      <Field label={t('manual.kind')}>
        <SegmentedControl
          value={kind}
          onChange={setKind}
          options={[
            { value: 'movie' as const, label: t('manual.kindMovie') },
            { value: 'episode' as const, label: t('manual.kindEpisode') },
            { value: 'season' as const, label: t('manual.kindSeason') },
          ]}
        />
      </Field>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-[1fr_100px]">
        <Field label={t('manual.titleLabel')} hint={t('manual.titleHint')}>
          <TextInput
            value={title}
            onChange={setTitle}
            placeholder="The Matrix"
            className="w-full min-w-0"
          />
        </Field>
        <Field label={t('manual.year')}>
          <TextInput
            value={year}
            onChange={setYear}
            placeholder="1999"
            className="w-full min-w-0"
          />
        </Field>
      </div>
      {kind !== 'movie' ? (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <Field label={t('manual.season')}>
            <TextInput
              value={season}
              onChange={setSeason}
              placeholder="1"
              className="w-full min-w-0"
            />
          </Field>
          {kind === 'episode' ? (
            <Field label={t('manual.episode')}>
              <TextInput
                value={episode}
                onChange={setEpisode}
                placeholder="1"
                className="w-full min-w-0"
              />
            </Field>
          ) : null}
        </div>
      ) : null}

      {error ? <p className="mt-1 text-[13px] font-semibold text-[#EF8091]">{error}</p> : null}
      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={add}
        confirmLabel={busy ? t('manual.adding') : t('manual.add')}
        busy={busy}
        disabled={!canAdd}
      />
    </Modal>
  );
}

function FileRow({
  f,
  checked,
  onToggle,
}: Readonly<{ f: TorrentFileView; checked: boolean; onToggle: () => void }>) {
  const label =
    f.episode != null
      ? `S${String(f.season ?? 0).padStart(2, '0')}E${String(f.episode).padStart(2, '0')}`
      : null;
  return (
    <label
      className={`flex items-center gap-2.5 rounded-lg px-2 py-1.5 ${f.isVideo ? 'cursor-pointer hover:bg-white/[0.03]' : 'opacity-45'}`}
    >
      <input
        type="checkbox"
        checked={checked}
        disabled={!f.isVideo}
        onChange={onToggle}
        className="h-3.5 w-3.5 accent-(--luma-accent)"
      />
      <span
        className="min-w-0 flex-1 truncate text-[12px] font-medium text-white/75"
        title={f.path}
      >
        {f.path.split('/').pop()}
      </span>
      {label ? (
        <span className="shrink-0 text-[11px] font-bold text-[#86A8FF]">{label}</span>
      ) : null}
      <span className="shrink-0 text-[11px] tabular-nums text-dim">{formatBytes(f.sizeBytes)}</span>
    </label>
  );
}

function ResultRow({ r, onPick }: Readonly<{ r: ManualReleaseView; onPick: () => void }>) {
  const t = useT();
  return (
    <button
      type="button"
      onClick={onPick}
      className="flex w-full items-center gap-3 border-b border-white/[0.04] px-3 py-2 text-left last:border-0 hover:bg-white/[0.03]"
    >
      <div className="min-w-0 flex-1">
        <div className="truncate text-[12.5px] font-semibold" title={r.title}>
          {r.title}
        </div>
        <div className="mt-0.5 flex flex-wrap items-center gap-x-2.5 text-[11px] font-semibold text-dim">
          <span>{r.indexerName}</span>
          {r.resolution ? <span className="text-[#86A8FF]">{r.resolution}</span> : null}
          {r.codec ? <span className="text-[#C792EA]">{r.codec}</span> : null}
          {r.sizeBytes != null ? <span>{formatBytes(r.sizeBytes)}</span> : null}
          {r.seeders != null ? (
            <span className="text-[#46D08D]">
              {t('requests.seedersN', { n: String(r.seeders) })}
            </span>
          ) : null}
          {r.detailsUrl ? (
            <span className="text-white/30">· {t('downloads.hasTrackerPage')}</span>
          ) : null}
        </div>
      </div>
      <IconDownload size={15} stroke={2.2} className="shrink-0 text-accent" />
    </button>
  );
}
