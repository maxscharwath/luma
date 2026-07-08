import {
  apiErrorText,
  GEN_LANGS as LANGS,
  GEN_QUALITIES as QUALITIES,
  type GenQuality,
  type SubCapabilities,
} from '@luma/core';
import { useT } from '@luma/ui';
import { useMemo, useState } from 'react';
import { IconClose, IconSparkles } from '#web/features/playback/icons';
import { lumaClient, type MovieView, type SubtitleView } from '#web/shared/lib/api';
import { Select } from '#web/shared/ui';

type Mode = 'transcribe' | 'translate';

const FIELD = 'text-[12px] font-bold uppercase tracking-[.12em] text-white/45';

/** The "Generate a subtitle" sheet: two modes (Whisper transcription / LLM
 * translation), a language + (transcribe) model-quality picker. Kicks off a
 * background generation and closes; the AV drawer then shows live progress. */
export function SubtitleGenerate({
  item,
  subs,
  caps,
  onStarted,
  onClose,
}: Readonly<{
  item: MovieView;
  subs: SubtitleView[];
  caps: SubCapabilities | null;
  onStarted: () => void;
  onClose: () => void;
}>) {
  const t = useT();
  const sources = useMemo(() => subs.filter((s) => s.url), [subs]);
  const [mode, setMode] = useState<Mode>(caps?.transcribe ? 'transcribe' : 'translate');
  const [lang, setLang] = useState('fr');
  const [quality, setQuality] = useState<GenQuality>('balanced');
  const [source, setSource] = useState<number>(sources[0]?.index ?? -1);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const qualityLabel = (q: GenQuality) =>
    q === 'fast' ? t('player.subQualityFast') : q === 'accurate' ? t('player.subQualityAccurate') : t('player.subQualityBalanced');

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const target = LANGS.find((l) => l.code === lang) ?? LANGS[0]!;
      if (mode === 'transcribe') {
        await lumaClient().generateSubtitle(item.id, {
          mode: 'transcribe',
          lang: target.label,
          spokenLang: target.code,
          quality,
        });
      } else {
        const src = sources.find((s) => s.index === source);
        if (!src) {
          setError(t('player.subNoSource'));
          setBusy(false);
          return;
        }
        await lumaClient().generateSubtitle(item.id, {
          mode: 'translate',
          lang: target.label,
          ...(src.subId ? { sourceSubId: src.subId } : { sourceTrack: src.index }),
        });
      }
      onStarted();
      onClose();
    } catch (e: unknown) {
      setError(apiErrorText(e, t('player.subGenError')));
    } finally {
      setBusy(false);
    }
  };

  const Tab = ({ m, label, hint }: { m: Mode; label: string; hint: string }) => {
    const enabled = m === 'transcribe' ? caps?.transcribe : caps?.translate;
    const on = mode === m;
    return (
      <button
        onClick={() => enabled && setMode(m)}
        disabled={!enabled}
        className={`flex-1 rounded-lg px-3 py-2.5 text-left transition-colors
          ${on ? 'bg-accent text-accent-ink' : 'bg-white/4 text-white/75 hover:bg-white/8'}
          ${enabled ? '' : 'cursor-not-allowed opacity-40'}`}
      >
        <div className="text-[14px] font-bold">{label}</div>
        <div className={`text-[11px] ${on ? 'text-accent-ink/75' : 'text-white/40'}`}>{hint}</div>
      </button>
    );
  };

  return (
    <div className="mb-4 rounded-2xl border border-accent/25 bg-[rgba(20,20,28,.6)] p-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-[15px] font-bold text-text">{t('player.subGenerate')}</h3>
        <button
          onClick={onClose}
          className="flex h-7 w-7 items-center justify-center rounded-full bg-white/8 text-white hover:bg-white/16"
          aria-label={t('player.subGenClose')}
        >
          <IconClose size={15} />
        </button>
      </div>

      <div className="mb-4 flex gap-2">
        <Tab m="transcribe" label={t('player.subModeTranscribe')} hint={t('player.subModeTranscribeHint')} />
        <Tab m="translate" label={t('player.subModeTranslate')} hint={t('player.subModeTranslateHint')} />
      </div>

      {mode === 'translate' ? (
        <div className="mb-4">
          <div className={`mb-2 ${FIELD}`}>{t('player.subSource')}</div>
          {sources.length ? (
            <Select
              value={String(source)}
              onChange={(v) => setSource(Number(v))}
              ariaLabel={t('player.subSource')}
              block
              options={sources.map((s) => ({
                value: String(s.index),
                label:
                  s.label ||
                  (s.language ?? '').toUpperCase() ||
                  t('player.subtitleTrack', { number: s.index + 1 }),
              }))}
            />
          ) : (
            <div className="text-[13px] text-white/45">{t('player.subNoSource')}</div>
          )}
        </div>
      ) : null}

      <div className="mb-4">
        <div className={`mb-2 ${FIELD}`}>{mode === 'transcribe' ? t('player.subSpokenLang') : t('player.subtitles')}</div>
        <Select
          value={lang}
          onChange={setLang}
          ariaLabel={mode === 'transcribe' ? t('player.subSpokenLang') : t('player.subtitles')}
          block
          options={LANGS.map((l) => ({ value: l.code, label: l.label }))}
        />
      </div>

      {mode === 'transcribe' ? (
        <div className="mb-4">
          <div className={`mb-2 ${FIELD}`}>{t('player.subQuality')}</div>
          <div className="flex gap-1.5 rounded-md bg-[#1A1A20] p-1">
            {QUALITIES.map((q) => (
              <button
                key={q}
                onClick={() => setQuality(q)}
                className={`flex-1 rounded-[7px] px-3 py-2 text-[13px] font-semibold transition-colors
                  ${quality === q ? 'bg-accent text-accent-ink' : 'text-white/70 hover:text-white'}`}
              >
                {qualityLabel(q)}
              </button>
            ))}
          </div>
        </div>
      ) : null}

      <p className="mb-3 text-[12px] leading-relaxed text-white/40">{t('player.subGenBackground')}</p>
      {error ? <div className="mb-3 rounded-lg bg-red-500/15 px-3 py-2 text-[13px] text-red-300">{error}</div> : null}
      <button
        onClick={() => void run()}
        disabled={busy || (mode === 'translate' && !sources.length)}
        className="flex w-full items-center justify-center gap-2 rounded-xl bg-accent px-4 py-3 text-[14px] font-bold text-accent-ink transition-opacity hover:opacity-90 disabled:opacity-50"
      >
        <IconSparkles size={16} />
        {t('player.subGenStart')}
      </button>
    </div>
  );
}
