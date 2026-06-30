import { type AudioTrack, channelLabel } from '@luma/core';
import { useT } from '@luma/ui';
import { langCode, langName } from '#tv/features/playback/player/fmt';
import { CheckGlyph } from '#tv/features/playback/player/icons';
import type { SubView } from '#tv/features/playback/player/useSubtitleSelection';

const TRACK = 'flex items-center gap-3.5 rounded-xl border border-transparent px-4 py-3.5';
const CODE =
  'min-w-9 rounded-md bg-[rgba(255,255,255,0.08)] py-1.25 text-center font-sans text-[12px] font-bold text-[rgba(244,243,240,0.85)]';
const LABEL = 'mb-3 mt-5.5 font-sans text-[12px] font-bold uppercase tracking-[0.14em] text-dim';
const FOCUSED = 'bg-[rgba(255,255,255,0.1)] shadow-(--ring-focus-sm)';
const RESTING = 'bg-[rgba(255,255,255,0.04)]';

/** Right-side Audio & Sous-titres drawer. Both the audio tracks and the
 * subtitles are selectable lists; a single `focus` index walks them in order
 * (audio rows first, then subtitle rows) see `usePlayerControls`. */
export function AvPanel({
  audioTracks,
  audioActive,
  rendered,
  options,
  active,
  focus,
}: Readonly<{
  audioTracks: AudioTrack[];
  audioActive: number;
  rendered: SubView[];
  options: (number | null)[];
  active: number | null;
  focus: number;
}>) {
  const t = useT();
  // Subtitle rows follow the audio rows in the shared focus order.
  const subOffset = audioTracks.length;
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
          else if (sv?.language) label = langName(t, sv.language) ?? sv.language.toUpperCase();
          else label = t('player.subtitleTrack', { number: opt + 1 });
          return (
            <div
              key={opt ?? 'off'}
              className={`${TRACK} ${focus === subOffset + i ? FOCUSED : RESTING}`}
            >
              <span className={CODE}>{opt == null ? '-' : langCode(sv?.language ?? null)}</span>
              <span className="flex-1 font-sans text-[15px] font-semibold text-text">{label}</span>
              {active === opt ? <CheckGlyph /> : null}
            </div>
          );
        })}
        {rendered.length === 0 ? (
          <div className="px-1 py-2 font-sans text-[14px] font-medium text-dim">
            {t('player.noSubtitles')}
          </div>
        ) : null}
      </div>
    </div>
  );
}
