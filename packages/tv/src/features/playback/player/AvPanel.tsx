import {
  type AudioTrack,
  channelLabel,
  type SubtitleGeneration,
  subtitleEtaTime,
  subtitleStageKey,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import { langCode, langName } from '#tv/features/playback/player/fmt';
import { GeneratePanel } from '#tv/features/playback/player/GeneratePanel';
import { CheckGlyph, SparkleGlyph, TrashGlyph } from '#tv/features/playback/player/icons';
import type { GenForm } from '#tv/features/playback/player/useSubtitleGen';
import type { SubView } from '#tv/features/playback/player/useSubtitleSelection';

const TRACK = 'flex items-center gap-3.5 rounded-xl border border-transparent px-4 py-3.5';
const CODE =
  'min-w-9 self-start rounded-md bg-[rgba(255,255,255,0.08)] py-1.25 text-center font-sans text-[12px] font-bold text-[rgba(244,243,240,0.85)]';
const LABEL = 'mb-3 mt-5.5 font-sans text-[12px] font-bold uppercase tracking-[0.14em] text-dim';
const FOCUSED = 'bg-[rgba(255,255,255,0.1)] shadow-(--ring-focus-sm)';
const RESTING = 'bg-[rgba(255,255,255,0.04)]';
// Generation / "IA" elements use a distinct violet accent (the selection accent
// stays gold). Tailwind v4 here = rgba()/hex literals, never `/opacity` modifiers.
const AI_BADGE =
  'flex items-center gap-1 rounded bg-[rgba(139,124,240,0.18)] px-1.5 py-0.5 font-sans text-[10px] font-bold text-[#c0b6f7]';

/** Right-side Audio & Sous-titres drawer. A single `focus` index walks the audio
 * rows, then the subtitle rows, then any running-generation (cancel) rows, then
 * the "create a missing subtitle" row see `usePlayerControls`. When `genOpen`,
 * the create row is replaced by the generate sheet, which owns the remote. */
export function AvPanel({
  audioTracks,
  audioActive,
  rendered,
  options,
  active,
  focus,
  pending,
  canCreate,
  genOpen,
  genFocus,
  form,
}: Readonly<{
  audioTracks: AudioTrack[];
  audioActive: number;
  rendered: SubView[];
  options: (number | null)[];
  active: number | null;
  focus: number;
  pending: SubtitleGeneration[];
  canCreate: boolean;
  genOpen: boolean;
  genFocus: number;
  form: GenForm;
}>) {
  const t = useT();
  const subOffset = audioTracks.length;
  const genOffset = subOffset + options.length;
  const createIndex = genOffset + pending.length;
  const sources = rendered.filter((s) => s.url);

  return (
    <div className="absolute inset-y-0 right-0 w-100 overflow-y-auto border-l border-border bg-[rgba(16,16,20,0.92)] px-7 py-7.5 backdrop-blur-xl animate-[tv-panel-in_0.3s_ease]">
      <div className="mb-6.5 font-display text-[22px] font-bold">{t('player.audioSubtitles')}</div>

      <div className={LABEL}>{t('player.audioTracks')}</div>
      <div className="flex flex-col gap-2">
        {audioTracks.map((a, i) => {
          const ch = channelLabel(a.channels);
          return (
            <div key={a.index} className={`${TRACK} ${focus === i ? FOCUSED : RESTING}`}>
              <span className={CODE}>{langCode(a.language ?? null)}</span>
              <span className="flex-1 font-sans text-[15px] font-semibold text-text">
                {a.title?.trim() || a.codec.toUpperCase()}
                {ch ? ` · ${ch}` : ''}
              </span>
              {audioActive === a.index ? <CheckGlyph /> : null}
            </div>
          );
        })}
        {audioTracks.length === 0 ? (
          <div className="px-1 py-2 font-sans text-[14px] font-medium text-dim">
            {t('player.noAudioTracks')}
          </div>
        ) : null}
      </div>

      <div className={LABEL}>{t('player.subtitles')}</div>
      <div className="flex flex-col gap-2">
        {options.map((opt, i) => {
          const sv = opt == null ? null : (rendered.find((s) => s.index === opt) ?? null);
          let label: string;
          if (opt == null) label = t('player.subtitlesOff');
          else if (sv?.ai && sv.label) label = sv.label;
          else if (sv?.language) label = langName(t, sv.language) ?? sv.language.toUpperCase();
          else label = t('player.subtitleTrack', { number: opt + 1 });
          return (
            <div
              key={opt ?? 'off'}
              className={`${TRACK} ${focus === subOffset + i ? FOCUSED : RESTING}`}
            >
              <span className={CODE}>{opt == null ? '-' : langCode(sv?.language ?? null)}</span>
              <span className="flex-1 font-sans text-[15px] font-semibold text-text">{label}</span>
              {sv?.ai ? (
                <span className={AI_BADGE}>
                  <SparkleGlyph />
                  IA
                </span>
              ) : null}
              {active === opt ? <CheckGlyph /> : null}
            </div>
          );
        })}
        {rendered.length === 0 ? (
          <div className="px-1 py-2 font-sans text-[14px] font-medium text-dim">
            {t('player.noSubtitles')}
          </div>
        ) : null}

        {/* Running generations: a live progress row, OK cancels the focused one. */}
        {pending.map((g, i) => (
          <GenRow key={g.id} gen={g} focused={focus === genOffset + i} t={t} />
        ))}
      </div>

      {/* Create a missing subtitle, or the generate sheet when it is open. */}
      {canCreate && genOpen ? (
        <GeneratePanel form={form} sources={sources} genFocus={genFocus} />
      ) : null}
      {canCreate && !genOpen ? (
        <div
          className={`mt-3 flex items-center justify-center gap-2 rounded-xl border border-dashed px-4 py-3.5 ${focus === createIndex ? 'border-[rgba(124,111,240,0.75)] bg-[rgba(124,111,240,0.14)] shadow-(--ring-focus-sm)' : 'border-[rgba(124,111,240,0.45)]'}`}
        >
          <span className="text-[#b3a9f5]">
            <SparkleGlyph />
          </span>
          <span className="font-sans text-[15px] font-bold text-[#b3a9f5]">
            {t('player.subCreateMissing')}
          </span>
        </div>
      ) : null}
    </div>
  );
}

/** One running-generation row, in the violet "IA" treatment: language + target,
 * the engine + stage + percent, a violet progress bar + ETA, and a trash glyph
 * (OK on the focused row cancels/discards it). */
function GenRow({
  gen,
  focused,
  t,
}: Readonly<{ gen: SubtitleGeneration; focused: boolean; t: ReturnType<typeof useT> }>) {
  const pct = Math.round(gen.progress * 100);
  const err = gen.status === 'error';
  const engine = gen.mode === 'translate' ? t('player.subAiBadge') : 'Whisper';
  return (
    <div
      className={`flex items-start gap-3.5 rounded-xl border px-4 py-3.5 ${focused ? 'border-[rgba(124,111,240,0.75)] bg-[rgba(124,111,240,0.14)] shadow-(--ring-focus-sm)' : 'border-[rgba(124,111,240,0.4)] bg-[rgba(124,111,240,0.06)]'}`}
    >
      <span className={CODE}>{langCode(gen.lang ?? null)}</span>
      <div className="flex-1">
        <div className="flex items-center gap-2">
          <span className="flex-1 font-sans text-[15px] font-semibold text-text">
            {gen.lang ?? ''}
          </span>
          <span className={AI_BADGE}>
            <SparkleGlyph />
            IA
          </span>
          <span className="text-dim">
            <TrashGlyph />
          </span>
        </div>
        <div className="mt-1 flex items-center justify-between font-sans text-[12px]">
          <span className={`flex items-center gap-2 ${err ? 'text-[#f0a0a0]' : 'text-[#9a8ff0]'}`}>
            {!err ? <span className="h-1.5 w-1.5 rounded-full bg-[#8b7ff0]" /> : null}
            {err ? t(subtitleStageKey(gen.stage)) : `${engine} · ${t(subtitleStageKey(gen.stage))}`}
          </span>
          <span className="font-bold text-[#b3a9f5]">{err ? '' : `${pct} %`}</span>
        </div>
        {!err ? (
          <>
            <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-[rgba(255,255,255,0.1)]">
              <div
                className="h-full rounded-full bg-[#7c6ff5] transition-[width] duration-500"
                style={{ width: `${pct}%` }}
              />
            </div>
            {gen.etaSec != null ? (
              <div className="mt-1.5 font-sans text-[11px] text-dim">
                {t('player.subEta', { time: subtitleEtaTime(gen.etaSec) })}
              </div>
            ) : null}
          </>
        ) : null}
      </div>
    </div>
  );
}
