// "Signaler un probleme" dialog: any logged-in user flags an issue on a movie /
// show / episode (wrong metadata, audio, video, subtitles, other) with an
// optional note. Posts to /api/reports; the server resolves the title itself.

import {
  apiErrorText,
  type MessageKey,
  type ReportCategory,
  type ReportSubjectKind,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import {
  IconCheck,
  IconDotsCircleHorizontal,
  IconInfoCircle,
  IconLoader2,
  IconMessage,
  IconVideo,
  IconVolume,
  IconX,
} from '@tabler/icons-react';
import { type ComponentType, useState } from 'react';
import { createCallable } from 'react-call';
import { useAuth } from '#web/shared/lib/auth';

interface CategoryMeta {
  key: ReportCategory;
  labelKey: MessageKey;
  descKey: MessageKey;
  Icon: ComponentType<{ size?: number; stroke?: number }>;
}

/** The five report categories, in display order. `metadata` = a wrong fiche. */
const CATEGORIES: readonly CategoryMeta[] = [
  {
    key: 'metadata',
    labelKey: 'report.category.metadata',
    descKey: 'report.category.metadataHint',
    Icon: IconInfoCircle,
  },
  {
    key: 'video',
    labelKey: 'report.category.video',
    descKey: 'report.category.videoHint',
    Icon: IconVideo,
  },
  {
    key: 'audio',
    labelKey: 'report.category.audio',
    descKey: 'report.category.audioHint',
    Icon: IconVolume,
  },
  {
    key: 'subtitles',
    labelKey: 'report.category.subtitles',
    descKey: 'report.category.subtitlesHint',
    Icon: IconMessage,
  },
  {
    key: 'other',
    labelKey: 'report.category.other',
    descKey: 'report.category.otherHint',
    Icon: IconDotsCircleHorizontal,
  },
];

// Open with `await ReportDialog.call({ subjectKind, subjectId, subjectTitle })`.
// The server resolves the title itself and there is nothing for the caller to
// refresh, so it resolves (`void`) purely on dismiss (including after a
// successful submit). Its root is mounted once by `CatalogModalHosts`.
export const ReportDialog = createCallable<
  { subjectKind: ReportSubjectKind; subjectId: string; subjectTitle: string },
  void
>(({ call, subjectKind, subjectId, subjectTitle }) => {
  const t = useT();
  const { client } = useAuth();
  const [category, setCategory] = useState<ReportCategory>('metadata');
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sent, setSent] = useState(false);

  const submit = () => {
    setBusy(true);
    setError(null);
    client
      .createReport({ subjectKind, subjectId, category, message: message.trim() || undefined })
      .then(() => setSent(true))
      .catch((e) => setError(apiErrorText(e, t('report.failed'))))
      .finally(() => setBusy(false));
  };

  return (
    <>
      <button
        type="button"
        aria-label={t('common.close')}
        onClick={() => call.end()}
        className="fixed inset-0 z-60 bg-[rgba(4,4,6,.66)] backdrop-blur-[3px]"
      />
      <div className="pointer-events-none fixed inset-0 z-61 flex items-center justify-center p-4">
        <section className="pointer-events-auto flex max-h-[88vh] w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-white/10 bg-[#0E0E12] shadow-[0_30px_90px_rgba(0,0,0,.6)]">
          <header className="flex items-start justify-between gap-4 border-b border-white/[0.07] px-7 py-5">
            <div className="min-w-0">
              <div className="text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
                {t('report.title')}
              </div>
              <h2 className="mt-1 truncate font-display text-[20px] font-bold">{subjectTitle}</h2>
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

          {sent ? (
            <div className="flex flex-col items-center gap-4 px-7 py-12 text-center">
              <span className="flex h-14 w-14 items-center justify-center rounded-full bg-[#46D08D]/15 text-[#46D08D]">
                <IconCheck size={30} stroke={2.4} />
              </span>
              <p className="text-[15px] font-semibold text-white/85">{t('report.submitted')}</p>
              <button
                type="button"
                onClick={() => call.end()}
                className="rounded-xl bg-accent px-5 py-2.5 text-[13.5px] font-bold text-[#0A0A0C] transition-colors hover:bg-accent-hover"
              >
                {t('common.close')}
              </button>
            </div>
          ) : (
            <div className="flex-1 space-y-5 overflow-y-auto px-7 py-5">
              <div>
                <div className="mb-2.5 text-[10px] font-bold uppercase tracking-[.12em] text-white/40">
                  {t('report.category')}
                </div>
                <div className="grid gap-2">
                  {CATEGORIES.map((c) => {
                    const on = c.key === category;
                    return (
                      <button
                        key={c.key}
                        type="button"
                        onClick={() => setCategory(c.key)}
                        aria-pressed={on}
                        className={`flex items-start gap-3 rounded-xl border px-3.5 py-3 text-left transition-colors ${
                          on
                            ? 'border-accent/45 bg-accent/[0.12]'
                            : 'border-white/8 bg-[#15151A] hover:bg-[#1a1a20]'
                        }`}
                      >
                        <span className={`mt-0.5 shrink-0 ${on ? 'text-accent' : 'text-white/45'}`}>
                          <c.Icon size={19} stroke={1.9} />
                        </span>
                        <span className="min-w-0">
                          <span className="block text-[13.5px] font-semibold text-white">
                            {t(c.labelKey)}
                          </span>
                          <span className="mt-0.5 block text-[12px] font-medium leading-snug text-white/45">
                            {t(c.descKey)}
                          </span>
                        </span>
                      </button>
                    );
                  })}
                </div>
              </div>

              <div>
                <label
                  htmlFor="report-message"
                  className="mb-2 block text-[10px] font-bold uppercase tracking-[.12em] text-white/40"
                >
                  {t('report.message')}
                </label>
                <textarea
                  id="report-message"
                  value={message}
                  onChange={(e) => setMessage(e.target.value)}
                  rows={3}
                  maxLength={2000}
                  placeholder={t('report.messagePlaceholder')}
                  className="w-full resize-none rounded-xl border border-white/12 bg-[#15151A] px-3.5 py-3 text-[13.5px] font-medium text-white outline-none placeholder:text-white/35 focus:border-white/25"
                />
              </div>

              {error ? (
                <div className="rounded-lg border border-[#E8536A]/18 bg-[#E8536A]/8 px-3.5 py-2.5 text-[12.5px] font-semibold text-[#EF8091]">
                  {error}
                </div>
              ) : null}

              <div className="flex gap-2.5">
                <button
                  type="button"
                  disabled={busy}
                  onClick={submit}
                  className="flex flex-1 items-center justify-center gap-2 rounded-xl bg-accent px-4 py-3 text-[13.5px] font-bold text-[#0A0A0C] transition-colors hover:bg-accent-hover disabled:opacity-60"
                >
                  {busy ? <IconLoader2 size={15} stroke={2.4} className="animate-spin" /> : null}
                  {t('report.submit')}
                </button>
                <button
                  type="button"
                  onClick={() => call.end()}
                  className="rounded-xl border border-white/12 bg-[#1A1A20] px-4 py-3 text-[13.5px] font-semibold text-white/75 hover:bg-[#222229]"
                >
                  {t('common.cancel')}
                </button>
              </div>
            </div>
          )}
        </section>
      </div>
    </>
  );
});
