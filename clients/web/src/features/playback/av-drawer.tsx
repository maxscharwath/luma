import type {
  AudioTrack,
  DownloadedSub,
  SubCapabilities,
  SubtitleGeneration,
  Translate,
} from '@luma/core';
import { channelLabel, subtitleEtaTime, subtitleStageKey } from '@luma/core';
import { useT } from '@luma/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { langName } from '#web/features/catalog/detail';
import { IconCheck, IconClose, IconSparkles, IconTrash } from '#web/features/playback/icons';
import { SubtitleGenerate } from '#web/features/playback/subtitle-generate';
import {
  SUB_COLORS,
  type SubEdge,
  type SubSize,
  type SubtitleStyle,
  subtitleCss,
} from '#web/features/playback/subtitle-style';
import { useSubtitleGenerations } from '#web/features/playback/use-subtitle-generations';
import type { MovieView, SubtitleView } from '#web/shared/lib/api';
import { lumaClient } from '#web/shared/lib/api';

/** Track language name, or the localized "Unknown" when no code is present. */
function trackLang(t: Translate, code: string | null): string {
  return langName(t, code) ?? t('player.langUnknown');
}

const SECTION = 'mb-3.5 text-[12px] font-bold uppercase tracking-[.14em] text-white/45';

function Row({
  code,
  label,
  tag,
  ai,
  active,
  disabled,
  onClick,
}: Readonly<{
  code: string;
  label: string;
  tag?: string;
  /** Generated (Whisper/translate) track: show the "IA" badge. */
  ai?: boolean;
  active: boolean;
  disabled?: boolean;
  onClick?: () => void;
}>) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`flex w-full items-center gap-3.5 rounded-xl border px-4 py-3.5 text-left transition-colors
        ${active ? 'border-accent/40 bg-accent-soft' : 'border-white/10 bg-white/4 hover:bg-white/8'}
        ${disabled ? 'cursor-not-allowed opacity-40' : 'cursor-pointer'}`}
    >
      <span className="min-w-9 rounded-md bg-white/8 py-1.5 text-center text-[12px] font-bold text-white/85">
        {code}
      </span>
      <span className="flex-1 text-[15px] font-semibold text-text">{label}</span>
      {ai ? (
        <span className="flex items-center gap-1 rounded bg-[rgba(139,124,240,0.18)] px-1.5 py-0.75 text-[10px] font-bold text-[#c0b6f7]">
          <IconSparkles size={11} />
          IA
        </span>
      ) : null}
      {!ai && tag ? (
        <span className="rounded bg-white/8 px-2 py-0.75 text-[10px] font-bold text-white/70">
          {tag}
        </span>
      ) : null}
      {active ? (
        <span className="text-accent">
          <IconCheck size={20} />
        </span>
      ) : null}
    </button>
  );
}

/** A live generation row, in the violet "IA" treatment: language + target, the
 * engine + stage + percent, a violet progress bar + ETA, and a trash control that
 * cancels/discards it. */
function GenRow({
  gen,
  t,
  onCancel,
}: Readonly<{ gen: SubtitleGeneration; t: Translate; onCancel: () => void }>) {
  const pct = Math.round(gen.progress * 100);
  const err = gen.status === 'error';
  const engine = gen.mode === 'translate' ? t('player.subAiBadge') : 'Whisper';
  return (
    <div className="rounded-xl border border-[rgba(124,111,240,0.4)] bg-[rgba(124,111,240,0.06)] px-4 py-3">
      <div className="flex items-center gap-3.5">
        <span className="min-w-9 rounded-md bg-white/8 py-1.5 text-center text-[12px] font-bold text-white/85">
          {(gen.lang ?? 'ST').toUpperCase().slice(0, 3)}
        </span>
        <span className="flex-1 text-[15px] font-semibold text-text">{gen.lang ?? ''}</span>
        <span className="flex items-center gap-1 rounded bg-[rgba(139,124,240,0.18)] px-1.5 py-0.75 text-[10px] font-bold text-[#c0b6f7]">
          <IconSparkles size={11} />
          IA
        </span>
        <button
          type="button"
          onClick={onCancel}
          aria-label={t('player.subGenCancel')}
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-white/40 hover:bg-red-500/15 hover:text-red-300"
        >
          <IconTrash size={16} />
        </button>
      </div>
      <div className="mt-2 flex items-center justify-between text-[11.5px]">
        <span
          className={`flex items-center gap-2 ${err ? 'text-red-300' : 'text-[#9a8ff0]'}`}
          title={err ? (gen.error ?? undefined) : undefined}
        >
          {!err ? <span className="h-1.5 w-1.5 rounded-full bg-[#8b7ff0]" /> : null}
          {err
            ? (gen.error ?? t(subtitleStageKey(gen.stage)))
            : `${engine} · ${t(subtitleStageKey(gen.stage))}`}
        </span>
        <span className="font-semibold text-[#b3a9f5]">{err ? '' : `${pct} %`}</span>
      </div>
      {!err ? (
        <>
          <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-white/10">
            <div
              className="h-full rounded-full bg-[#7c6ff5] transition-[width] duration-500"
              style={{ width: `${pct}%` }}
            />
          </div>
          {gen.etaSec != null ? (
            <div className="mt-1.5 text-[11px] text-white/40">
              {t('player.subEta', { time: subtitleEtaTime(gen.etaSec) })}
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: Readonly<{
  value: T;
  options: { v: T; label: string }[];
  onChange: (v: T) => void;
}>) {
  return (
    <div className="flex gap-1.5 rounded-md bg-[#1A1A20] p-1">
      {options.map((o) => (
        <button
          key={o.v}
          type="button"
          onClick={() => onChange(o.v)}
          className={`rounded-[7px] px-3.5 py-2 text-[13px] font-semibold transition-colors
            ${value === o.v ? 'bg-accent text-accent-ink' : 'text-white/70 hover:text-white'}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

const SIZES: { v: SubSize; label: string }[] = [
  { v: 'sm', label: 'S' },
  { v: 'md', label: 'M' },
  { v: 'lg', label: 'L' },
  { v: 'xl', label: 'XL' },
];
/** Subtitle edge styles; labels are catalog keys translated at render. */
const EDGES: {
  v: SubEdge;
  labelKey: 'subtitle.shadow' | 'subtitle.outline' | 'subtitle.background' | 'subtitle.none';
}[] = [
  { v: 'shadow', labelKey: 'subtitle.shadow' },
  { v: 'outline', labelKey: 'subtitle.outline' },
  { v: 'box', labelKey: 'subtitle.background' },
  { v: 'none', labelKey: 'subtitle.none' },
];

/** Right-side audio + subtitle drawer (non-modal playback continues), with
 * the live subtitle-appearance controls. */
export function AvDrawer({
  item,
  subs,
  audioTracks,
  audioIndex,
  onPickAudio,
  activeSub,
  onPickSub,
  onDownloaded,
  onDeleteSub,
  subStyle,
  onStyleChange,
  onClose,
}: Readonly<{
  item: MovieView;
  /** Embedded + already-downloaded subtitle tracks. */
  subs: SubtitleView[];
  audioTracks: AudioTrack[];
  audioIndex: number;
  onPickAudio: (index: number) => void;
  activeSub: number | null;
  onPickSub: (index: number | null) => void;
  /** Called with each generated subtitle (so the parent merges it in). */
  onDownloaded: (sub: DownloadedSub) => void;
  /** Delete a generated subtitle track by its id. */
  onDeleteSub: (subId: string) => void;
  subStyle: SubtitleStyle;
  onStyleChange: (next: Partial<SubtitleStyle>) => void;
  onClose: () => void;
}>) {
  const t = useT();
  const [genOpen, setGenOpen] = useState(false);
  // Which generation actions this server build + config enable (hide empty UI).
  const [caps, setCaps] = useState<SubCapabilities | null>(null);
  useEffect(() => {
    let cancelled = false;
    lumaClient()
      .subtitleCapabilities(item.id)
      .then((c) => !cancelled && setCaps(c))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [item.id]);
  const canAi = Boolean(caps?.transcribe || caps?.translate);
  // Guard the async completion work against a drawer unmount.
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);
  // When a generation finishes, merge the freshly-cached list and auto-select
  // ONLY the track that generation produced (matched by id) never blindly flip
  // subtitles on to some other/last track.
  const onGenerationComplete = useCallback(
    (subId: string) => {
      lumaClient()
        .downloadedSubtitles(item.id)
        .then((list) => {
          if (!mountedRef.current) return;
          list.forEach(onDownloaded);
          const idx = list.findIndex((d) => d.id === subId);
          if (idx >= 0) onPickSub(1000 + idx);
        })
        .catch(() => undefined);
    },
    [item.id, onDownloaded, onPickSub],
  );
  const { gens, cancel, refresh } = useSubtitleGenerations(item.id, true, onGenerationComplete);
  // Kicking off a generation: merge the cached list and re-arm progress polling.
  const onGenerationStarted = useCallback(() => {
    lumaClient()
      .downloadedSubtitles(item.id)
      .then((list) => list.forEach(onDownloaded))
      .catch(() => undefined);
    refresh();
  }, [item.id, onDownloaded, refresh]);
  const pending = gens.filter((g) => g.status !== 'done');
  const edgeOptions = EDGES.map((e) => ({ v: e.v, label: t(e.labelKey) }));
  return (
    <>
      <button
        type="button"
        className="absolute inset-0 z-68 cursor-default bg-black/35"
        onClick={onClose}
        aria-label={t('common.close')}
      />
      <div className="absolute inset-y-0 right-0 z-69 w-[min(25rem,100vw)] overflow-y-auto border-l border-white/10 bg-[rgba(16,16,20,.94)] p-7 pb-[max(1.75rem,env(safe-area-inset-bottom))] backdrop-blur-2xl">
        <div className="mb-7 flex items-center justify-between">
          <h2 className="font-display text-[22px] font-bold text-text">
            {t('player.audioSubtitles')}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="flex h-9 w-9 items-center justify-center rounded-full bg-white/8 text-white hover:bg-white/16"
            aria-label={t('common.close')}
          >
            <IconClose />
          </button>
        </div>

        <div className={SECTION}>{t('player.audioTracks')}</div>
        <div className="mb-8 flex flex-col gap-2">
          {audioTracks.length > 0 ? (
            audioTracks.map((a) => {
              const ch = channelLabel(a.channels);
              return (
                <Row
                  key={a.index}
                  code={(a.language ?? '-').toUpperCase().slice(0, 3)}
                  label={a.title?.trim() || trackLang(t, a.language)}
                  tag={ch ? `${a.codec.toUpperCase()} · ${ch}` : a.codec.toUpperCase()}
                  active={audioIndex === a.index}
                  onClick={() => onPickAudio(a.index)}
                />
              );
            })
          ) : (
            <div className="text-[13px] text-white/45">{t('player.noAudioTracks')}</div>
          )}
        </div>

        <div className={SECTION}>{t('player.subtitles')}</div>
        <div className="mb-3 flex flex-col gap-2">
          <Row
            code="OFF"
            label={t('player.subtitlesOff')}
            active={activeSub == null}
            onClick={() => onPickSub(null)}
          />
          {subs.map((s) => {
            const selectable = Boolean(s.url);
            const ai = Boolean(s.downloaded);
            let tag: string | undefined;
            if (!ai) {
              tag = selectable
                ? s.codec.toUpperCase()
                : `${s.codec.toUpperCase()} · ${t('player.pictureSub')}`;
            }
            const row = (
              <Row
                key={s.index}
                code={(s.language ?? 'ST').toUpperCase().slice(0, 3)}
                label={ai && s.label ? s.label : trackLang(t, s.language)}
                tag={tag}
                ai={ai}
                active={activeSub === s.index}
                disabled={!selectable}
                onClick={selectable ? () => onPickSub(s.index) : undefined}
              />
            );
            return s.subId ? (
              <div key={s.index} className="flex items-center gap-2">
                <div className="flex-1">{row}</div>
                <button
                  type="button"
                  onClick={() => onDeleteSub(s.subId as string)}
                  aria-label={t('player.subGenDelete')}
                  className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-white/4 text-white/50 hover:bg-red-500/15 hover:text-red-300"
                >
                  <IconTrash size={16} />
                </button>
              </div>
            ) : (
              <div key={s.index}>{row}</div>
            );
          })}
          {pending.map((g) => (
            <GenRow key={g.id} gen={g} t={t} onCancel={() => cancel(g.id)} />
          ))}
        </div>
        {/* ---- create a missing subtitle on-device (Whisper / translate) ---- */}
        {canAi && genOpen ? (
          <SubtitleGenerate
            item={item}
            subs={subs}
            caps={caps}
            onStarted={onGenerationStarted}
            onClose={() => setGenOpen(false)}
          />
        ) : null}
        {canAi && !genOpen ? (
          <button
            type="button"
            onClick={() => setGenOpen(true)}
            className="mb-4 flex w-full items-center justify-center gap-2 rounded-xl border border-dashed border-[rgba(124,111,240,0.45)] px-4 py-3 text-[14px] font-semibold text-[#b3a9f5] transition-colors hover:bg-[rgba(124,111,240,0.1)]"
          >
            <IconSparkles size={15} />
            {t('player.subCreateMissing')}
          </button>
        ) : null}

        {/* ---- subtitle appearance ---- */}
        <div className={SECTION}>{t('player.subAppearance')}</div>
        <div className="mb-4 flex aspect-21/9 items-end justify-center overflow-hidden rounded-xl bg-linear-to-br from-[#2A2440] to-[#0E1226] pb-4">
          <span style={subtitleCss(subStyle)}>{t('player.subPreview')}</span>
        </div>
        <div className="flex flex-col gap-4">
          <div className="flex items-center justify-between gap-3">
            <span className="text-[14px] font-semibold text-text">{t('player.subSize')}</span>
            <Segmented
              value={subStyle.size}
              options={SIZES}
              onChange={(v) => onStyleChange({ size: v })}
            />
          </div>
          <div className="flex items-center justify-between gap-3">
            <span className="text-[14px] font-semibold text-text">{t('player.subColor')}</span>
            <div className="flex gap-2.5">
              {SUB_COLORS.map((c) => (
                <button
                  key={c}
                  type="button"
                  onClick={() => onStyleChange({ color: c })}
                  aria-label={c}
                  className="h-8 w-8 rounded-full"
                  style={{
                    background: c,
                    boxShadow:
                      subStyle.color === c
                        ? '0 0 0 2px var(--luma-accent)'
                        : '0 0 0 1px rgba(255,255,255,.2)',
                  }}
                />
              ))}
            </div>
          </div>
          <div className="flex items-center justify-between gap-3">
            <span className="text-[14px] font-semibold text-text">{t('player.subEdgeBg')}</span>
            <Segmented
              value={subStyle.edge}
              options={edgeOptions}
              onChange={(v) => onStyleChange({ edge: v })}
            />
          </div>
          {subStyle.edge === 'box' ? (
            <div className="flex items-center justify-between gap-4">
              <span className="whitespace-nowrap text-[14px] font-semibold text-text">
                {t('player.subBgOpacity')}
              </span>
              <div className="flex flex-1 items-center gap-3">
                <input
                  type="range"
                  min={0}
                  max={100}
                  value={subStyle.bgOpacity}
                  onChange={(e) => onStyleChange({ bgOpacity: Number(e.target.value) })}
                  className="flex-1 accent-accent"
                />
                <span className="w-10 text-right text-[13px] text-white/70">
                  {subStyle.bgOpacity}%
                </span>
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </>
  );
}
