import { apiErrorText, type DownloadedSub, type SubCapabilities } from '@luma/core';
import { useT } from '@luma/ui';
import { useState } from 'react';
import { lumaClient, type MovieView, type SubtitleView } from '#web/shared/lib/api';

/** AI subtitle generation panel inside the AV drawer: transcribe the audio
 * (Whisper) or translate an existing text track into the chosen language (LLM).
 * Each button shows only if the matching provider is configured (`caps`). Both
 * POST to /subtitles/generate. Slow — the buttons stay busy until the track
 * comes back. */
export function SubtitleGenerate({
  item,
  subs,
  caps,
  onDownloaded,
}: Readonly<{
  item: MovieView;
  subs: SubtitleView[];
  caps: SubCapabilities | null;
  onDownloaded: (s: DownloadedSub) => void;
}>) {
  const t = useT();
  const [lang, setLang] = useState('Français');
  const [busy, setBusy] = useState<'transcribe' | 'translate' | null>(null);
  const [error, setError] = useState<string | null>(null);

  // First available text track, used as the translation source.
  const source = subs.find((s) => s.url && !s.downloaded) ?? subs.find((s) => s.url);

  const run = async (mode: 'transcribe' | 'translate') => {
    setBusy(mode);
    setError(null);
    try {
      let sourceVtt: string | undefined;
      if (mode === 'translate') {
        if (!source?.url) return;
        sourceVtt = await fetch(source.url).then((r) => r.text());
      }
      const sub = await lumaClient().generateSubtitle(item.id, { lang: lang.trim() || 'English', sourceVtt });
      onDownloaded(sub);
    } catch (e: unknown) {
      setError(apiErrorText(e, t('player.subGenError')));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="mb-6 flex flex-col gap-2.5">
      <input
        value={lang}
        onChange={(e) => setLang(e.target.value)}
        placeholder={t('player.subLangPlaceholder')}
        className="rounded-lg border border-white/12 bg-white/4 px-3.5 py-2.5 text-[14px] text-text outline-none focus:border-accent/50"
      />
      <div className="flex gap-2">
        {caps?.transcribe ? (
          <button
            onClick={() => void run('transcribe')}
            disabled={busy !== null}
            className="flex-1 rounded-xl border border-white/12 bg-white/4 px-3 py-3 text-[13px] font-semibold text-white/80 transition-colors hover:bg-white/8 disabled:opacity-50"
          >
            {busy === 'transcribe' ? t('player.subGenerating') : t('player.subTranscribe')}
          </button>
        ) : null}
        {caps?.translate ? (
          <button
            onClick={() => void run('translate')}
            disabled={busy !== null || !source?.url}
            className="flex-1 rounded-xl border border-white/12 bg-white/4 px-3 py-3 text-[13px] font-semibold text-white/80 transition-colors hover:bg-white/8 disabled:opacity-50"
          >
            {busy === 'translate' ? t('player.subGenerating') : t('player.subTranslate')}
          </button>
        ) : null}
      </div>
      <p className="px-1 text-[12px] text-white/40">{t('player.subGenHint')}</p>
      {error ? (
        <div className="rounded-lg bg-red-500/15 px-3 py-2 text-[13px] text-red-300">{error}</div>
      ) : null}
    </div>
  );
}
