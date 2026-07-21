// "Media details" modal (admin): the technical truth about the file(s) backing
// one catalog item path, container, size, duration, and every video / audio /
// subtitle stream ffprobe found. All of this already rides on the item DTO the
// fiche loaded, so the modal reads from cache and adds no request.

import { channelLabel, codecLabel, langName, type MediaFile, type MediaItem } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconFileInfo, IconLoader2, IconX } from '@tabler/icons-react';
import { useQuery } from '@tanstack/react-query';
import { createCallable } from 'react-call';
import { formatBytes, formatDuration } from '#web/shared/lib/adminFormat';
import { catalogQueries } from '#web/shared/lib/queries';

// Open with `await MediaInfoModal.call({ id, title })`; read-only, so it resolves
// (`void`) purely on dismiss. Its root is mounted once by `CatalogModalHosts`.
export const MediaInfoModal = createCallable<{ id: string; title: string }, void>(
  ({ call, id, title }) => {
    const t = useT();
    // Cached: the fiche already loaded this item, so this resolves instantly.
    const { data: item, isPending } = useQuery(catalogQueries.item(id));
    const files = item ? filesOf(item) : [];

    return (
      <>
        <button
          type="button"
          aria-label={t('common.close')}
          onClick={() => call.end()}
          className="fixed inset-0 z-60 bg-[rgba(4,4,6,.66)] backdrop-blur-[3px]"
        />
        <div className="pointer-events-none fixed inset-0 z-61 flex items-center justify-center p-4">
          <section className="pointer-events-auto flex max-h-[88vh] w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-white/10 bg-[#0E0E12] shadow-[0_30px_90px_rgba(0,0,0,.6)]">
            <header className="flex items-start justify-between gap-4 border-b border-white/[0.07] px-7 py-5">
              <div className="min-w-0">
                <div className="text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
                  {t('mediaInfo.title')}
                </div>
                <h2 className="mt-1 truncate font-display text-[20px] font-bold">{title}</h2>
              </div>
              <button
                type="button"
                onClick={() => call.end()}
                aria-label={t('common.close')}
                className="shrink-0 rounded-xl border border-white/9 bg-[#15151A] px-2.5 py-2 text-white/60 hover:bg-[#1a1a20] hover:text-white"
              >
                <IconX size={18} stroke={2.1} />
              </button>
            </header>

            <div className="flex-1 space-y-5 overflow-y-auto px-7 py-5">
              {isPending ? (
                <div className="flex justify-center py-16 text-white/40">
                  <IconLoader2 size={26} stroke={2.2} className="animate-spin" />
                </div>
              ) : null}
              {!isPending && files.length === 0 ? (
                <p className="py-16 text-center text-[13px] text-white/40">
                  {t('mediaInfo.noFile')}
                </p>
              ) : null}
              {files.map((f, i) => (
                <FileCard key={f.id} file={f} index={i} multi={files.length > 1} />
              ))}
            </div>
          </section>
        </div>
      </>
    );
  },
);

/** The item's physical files, or a synthetic one from the top-level fields for
 * legacy rows that predate the per-file list. */
function filesOf(item: MediaItem): MediaFile[] {
  if (item.files.length) return item.files;
  return [
    {
      id: item.id,
      relPath: item.relPath ?? null,
      container: item.container,
      durationMs: item.durationMs ?? null,
      video: item.video ?? null,
      audio: item.audio ?? null,
      audioTracks: item.audioTracks ?? [],
      subtitles: item.subtitles ?? [],
      size: null,
      edition: null,
      probed: item.video != null,
    },
  ];
}

function FileCard({
  file,
  index,
  multi,
}: Readonly<{ file: MediaFile; index: number; multi: boolean }>) {
  const t = useT();
  const name = file.relPath?.split('/').pop() ?? t('mediaInfo.unknownFile');
  const fallbackAudio = file.audio ? [file.audio] : [];
  const audio = file.audioTracks.length > 0 ? file.audioTracks : fallbackAudio;
  return (
    <div className="overflow-hidden rounded-xl border border-white/8 bg-white/[0.02]">
      <div className="flex items-start gap-3 border-b border-white/[0.06] px-4 py-3">
        <IconFileInfo size={18} stroke={1.9} className="mt-0.5 shrink-0 text-white/40" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-[13.5px] font-semibold">{name}</span>
            {multi ? <Chip>#{index + 1}</Chip> : null}
            {file.edition ? <Chip accent>{file.edition}</Chip> : null}
          </div>
          {file.relPath ? (
            <div className="mt-0.5 break-all font-mono text-[11px] text-white/35">
              {file.relPath}
            </div>
          ) : null}
        </div>
      </div>

      <dl className="grid grid-cols-2 gap-x-5 gap-y-2.5 px-4 py-3.5 sm:grid-cols-3">
        <Field label={t('mediaInfo.container')} value={file.container.toUpperCase()} />
        <Field
          label={t('mediaInfo.size')}
          value={file.size != null ? formatBytes(file.size) : '-'}
        />
        <Field
          label={t('mediaInfo.duration')}
          value={file.durationMs != null ? formatDuration(file.durationMs) : '-'}
        />
      </dl>

      {!file.probed ? (
        <p className="border-t border-white/[0.06] px-4 py-3 text-[12px] text-amber-300/80">
          {t('mediaInfo.unprobed')}
        </p>
      ) : (
        <>
          <Section label={t('mediaInfo.video')}>
            {file.video ? (
              <TrackLine
                parts={[
                  codecLabel(file.video.codec),
                  file.video.width && file.video.height
                    ? `${file.video.width}×${file.video.height}`
                    : null,
                  file.video.hdr ? 'HDR' : null,
                  file.video.bitDepth ? `${file.video.bitDepth} ${t('mediaInfo.bit')}` : null,
                ]}
              />
            ) : (
              <Muted>{'-'}</Muted>
            )}
          </Section>

          <Section label={t('mediaInfo.audio')}>
            {audio.length ? (
              audio.map((a) => (
                <TrackLine
                  key={`audio-${a.index}`}
                  badge={a.default ? t('mediaInfo.default') : null}
                  parts={[
                    langName(t, a.language) ?? a.language ?? null,
                    codecLabel(a.codec),
                    channelLabel(a.channels),
                    a.title ?? null,
                  ]}
                />
              ))
            ) : (
              <Muted>{'-'}</Muted>
            )}
          </Section>

          <Section label={t('mediaInfo.subtitles')} last>
            {file.subtitles.length ? (
              file.subtitles.map((s) => (
                <TrackLine
                  key={`sub-${s.language ?? 'und'}-${s.codec}`}
                  parts={[langName(t, s.language) ?? s.language ?? null, codecLabel(s.codec)]}
                />
              ))
            ) : (
              <Muted>{t('mediaInfo.noSubs')}</Muted>
            )}
          </Section>
        </>
      )}
    </div>
  );
}

function Section({
  label,
  last,
  children,
}: Readonly<{ label: string; last?: boolean; children: React.ReactNode }>) {
  return (
    <div className={`px-4 py-3 ${last ? '' : 'border-b border-white/[0.06]'}`}>
      <div className="mb-1.5 text-[10px] font-bold uppercase tracking-[.12em] text-white/35">
        {label}
      </div>
      <div className="space-y-1">{children}</div>
    </div>
  );
}

/** A dot-separated technical line, skipping the parts that are unknown. */
function TrackLine({
  parts,
  badge,
}: Readonly<{ parts: (string | null | undefined)[]; badge?: string | null }>) {
  const shown = parts.filter((p): p is string => !!p && p.length > 0).join('  ·  ');
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[12.5px] text-white/75">
      {badge ? <Chip accent>{badge}</Chip> : null}
      <span>{shown}</span>
    </div>
  );
}

function Field({ label, value }: Readonly<{ label: string; value: string }>) {
  return (
    <div className="min-w-0">
      <dt className="text-[10px] font-bold uppercase tracking-[.1em] text-white/35">{label}</dt>
      <dd className="mt-0.5 truncate text-[13px] text-white/85">{value}</dd>
    </div>
  );
}

function Chip({ children, accent }: Readonly<{ children: React.ReactNode; accent?: boolean }>) {
  return (
    <span
      className={`rounded-full px-1.5 py-0.5 text-[10px] font-bold ${
        accent ? 'bg-accent/20 text-accent' : 'bg-white/8 text-white/50'
      }`}
    >
      {children}
    </span>
  );
}

function Muted({ children }: Readonly<{ children: React.ReactNode }>) {
  return <span className="text-[12.5px] text-white/35">{children}</span>;
}
