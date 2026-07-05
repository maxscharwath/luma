import { useT } from '@luma/ui';
import { langName } from '#tv/features/playback/player/fmt';
import {
  GEN_LANGS,
  GEN_QUALITIES,
  type GenForm,
} from '#tv/features/playback/player/useSubtitleGen';
import type { SubView } from '#tv/features/playback/player/useSubtitleSelection';

const FIELD = 'mb-2 mt-4 font-sans text-[11px] font-bold uppercase tracking-[0.14em] text-dim';
const BOX = 'rounded-xl px-4 py-3 font-sans text-[15px] font-semibold';
const FOCUSED = 'bg-[rgba(255,255,255,0.12)] shadow-(--ring-focus-sm)';
const RESTING = 'bg-[rgba(255,255,255,0.05)]';

/** The "Générer un sous-titre" sheet (D-pad driven). A single `genFocus` walks the
 * controls; ◀▶ change the focused control's value. Field order matches
 * `useSubtitleGen().fields`: mode(0), then [transcribe: lang, quality] /
 * [translate: source, lang], then the start button (last). */
export function GeneratePanel({
  form,
  sources,
  genFocus,
}: Readonly<{ form: GenForm; sources: SubView[]; genFocus: number }>) {
  const t = useT();
  const transcribe = form.mode === 'transcribe';
  // Field indices for the current mode (see useSubtitleGen.fields).
  const langIdx = transcribe ? 1 : 2;
  const qualityIdx = 2;
  const sourceIdx = 1;
  const startIdx = 3;

  const lang = GEN_LANGS[form.langIndex] ?? GEN_LANGS[0]!;
  const source = sources[form.sourceIndex];
  const qualityLabel = (q: (typeof GEN_QUALITIES)[number]) =>
    q === 'fast'
      ? t('player.subQualityFast')
      : q === 'accurate'
        ? t('player.subQualityAccurate')
        : t('player.subQualityBalanced');
  const sourceLabel = source
    ? source.label ||
      (source.language
        ? (langName(t, source.language) ?? source.language.toUpperCase())
        : t('player.subtitleTrack', { number: source.index + 1 }))
    : t('player.subNoSource');

  return (
    <div className="mt-3 rounded-2xl border border-[rgba(242,180,66,0.25)] bg-[rgba(20,20,28,0.6)] p-4">
      <div className="mb-3 font-display text-[16px] font-bold text-text">
        {t('player.subGenerate')}
      </div>

      {/* mode tabs (field 0) */}
      <div
        className={`flex gap-2 rounded-xl p-1 ${genFocus === 0 ? 'shadow-(--ring-focus-sm)' : ''}`}
      >
        {(['transcribe', 'translate'] as const).map((m) => {
          const on = form.mode === m;
          return (
            <div
              key={m}
              className={`flex-1 rounded-lg px-3 py-2 ${on ? 'bg-accent text-[#1a1206]' : 'bg-[rgba(255,255,255,0.05)] text-dim'}`}
            >
              <div className="font-sans text-[14px] font-bold">
                {m === 'transcribe' ? t('player.subModeTranscribe') : t('player.subModeTranslate')}
              </div>
              <div className="font-sans text-[11px] opacity-75">
                {m === 'transcribe'
                  ? t('player.subModeTranscribeHint')
                  : t('player.subModeTranslateHint')}
              </div>
            </div>
          );
        })}
      </div>

      {!transcribe ? (
        <>
          <div className={FIELD}>{t('player.subSource')}</div>
          <div className={`${BOX} ${genFocus === sourceIdx ? FOCUSED : RESTING} text-text`}>
            {sourceLabel}
          </div>
        </>
      ) : null}

      <div className={FIELD}>{transcribe ? t('player.subSpokenLang') : t('player.subtitles')}</div>
      <div className={`${BOX} ${genFocus === langIdx ? FOCUSED : RESTING} text-text`}>
        {lang.label}
      </div>

      {transcribe ? (
        <>
          <div className={FIELD}>{t('player.subQuality')}</div>
          <div
            className={`flex gap-2 rounded-xl p-1 ${genFocus === qualityIdx ? 'shadow-(--ring-focus-sm)' : ''}`}
          >
            {GEN_QUALITIES.map((q) => (
              <div
                key={q}
                className={`flex-1 rounded-lg px-3 py-2 text-center font-sans text-[13px] font-semibold ${form.quality === q ? 'bg-accent text-[#1a1206]' : 'bg-[rgba(255,255,255,0.05)] text-dim'}`}
              >
                {qualityLabel(q)}
              </div>
            ))}
          </div>
        </>
      ) : null}

      <p className="mt-4 font-sans text-[12px] leading-relaxed text-dim">
        {t('player.subGenBackground')}
      </p>
      <div
        className={`mt-3 rounded-xl px-4 py-3 text-center font-sans text-[15px] font-bold ${genFocus === startIdx ? 'bg-accent-bright text-[#1a1206] shadow-(--ring-focus-sm)' : 'bg-accent text-[#1a1206]'}`}
      >
        {t('player.subGenStart')}
      </div>
    </div>
  );
}
