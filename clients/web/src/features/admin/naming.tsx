// Admin "Nommage & organisation": edit the Sonarr/Radarr-style naming
// templates with a live sample, then preview + apply a library-wide rename.

import { apiErrorText, type NamingTemplatesView, type OrganizePlan } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconArrowRight, IconBraces, IconLoader2, IconWand } from '@tabler/icons-react';
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { NamingTokenModal } from '#web/features/admin/naming-tokens';
import { Denied, PageHeader, useCap } from '#web/features/admin/shell';
import { Card, Modal, ModalActions, Section } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';
import { Select } from '#web/shared/ui';

type FieldKey = Exclude<keyof NamingTemplatesView, 'case'>;

const FIELDS: { key: FieldKey; labelKey: string }[] = [
  { key: 'movieFolder', labelKey: 'naming.movieFolder' },
  { key: 'movieFile', labelKey: 'naming.movieFile' },
  { key: 'seriesFolder', labelKey: 'naming.seriesFolder' },
  { key: 'seasonFolder', labelKey: 'naming.seasonFolder' },
  { key: 'episodeFile', labelKey: 'naming.episodeFile' },
];

const CASES: { value: string; labelKey: string }[] = [
  { value: 'default', labelKey: 'naming.caseDefault' },
  { value: 'upper', labelKey: 'naming.caseUpper' },
  { value: 'lower', labelKey: 'naming.caseLower' },
];

export function NamingPage() {
  const t = useT();
  const { client } = useAuth();
  const canManage = useCap('library.manage');

  const [tpl, setTpl] = useState<NamingTemplatesView | null>(null);
  const [sample, setSample] = useState<{ movie: string; episode: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [openField, setOpenField] = useState<FieldKey | null>(null);

  useEffect(() => {
    client
      .adminNaming()
      .then((v) => {
        setTpl(v.templates);
        setSample(v.sample);
      })
      .catch(() => undefined);
  }, [client]);

  // Debounced live sample as the templates change.
  const debounce = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const refreshSample = useCallback(
    (next: NamingTemplatesView) => {
      clearTimeout(debounce.current);
      debounce.current = setTimeout(() => {
        client
          .namingSample(next)
          .then(setSample)
          .catch(() => undefined);
      }, 250);
    },
    [client],
  );

  const set = (key: keyof NamingTemplatesView, value: string) => {
    setTpl((prev) => {
      if (!prev) return prev;
      const next = { ...prev, [key]: value };
      refreshSample(next);
      setSaved(false);
      return next;
    });
  };
  // Keep the modal editing the live template value.
  const setField = (key: FieldKey) => (value: string) => set(key, value);

  const save = () => {
    if (!tpl) return;
    setSaving(true);
    client
      .saveNaming(tpl)
      .then(() => {
        setSaved(true);
        setTimeout(() => setSaved(false), 2500);
      })
      .finally(() => setSaving(false));
  };

  if (!canManage) return <Denied />;

  return (
    <>
      <PageHeader title={t('admin.namingTitle')} subtitle={t('admin.namingSub')} />

      <Card className="mt-6 p-6">
        {tpl ? (
          <div className="flex flex-col gap-4">
            {FIELDS.map((f) => (
              <label key={f.key} className="block">
                <span className="mb-1.5 block text-[12px] font-bold uppercase tracking-[.12em] text-dim">
                  {t(f.labelKey as Parameters<typeof t>[0])}
                </span>
                <div className="flex gap-2">
                  <input
                    value={tpl[f.key]}
                    onChange={(e) => set(f.key, e.target.value)}
                    className="w-full rounded-[9px] border border-border-strong bg-[#0F0F13] px-3.5 py-2.5 font-mono text-[13px] text-text outline-none focus:border-accent/60"
                  />
                  <button
                    type="button"
                    onClick={() => setOpenField(f.key)}
                    title={t('naming.tokensTitle')}
                    className="inline-flex shrink-0 items-center gap-1.5 rounded-[9px] border border-white/12 bg-[#1A1A20] px-3 text-[13px] font-semibold text-white/80 hover:bg-[#222229]"
                  >
                    <IconBraces size={15} stroke={2} />
                    {t('naming.tokens')}
                  </button>
                </div>
              </label>
            ))}

            <div className="block max-w-xs">
              <span className="mb-1.5 block text-[12px] font-bold uppercase tracking-[.12em] text-dim">
                {t('naming.caseLabel')}
              </span>
              <Select
                value={tpl.case}
                onChange={(v) => set('case', v)}
                ariaLabel={t('naming.caseLabel')}
                block
                options={CASES.map((c) => ({
                  value: c.value,
                  label: t(c.labelKey as Parameters<typeof t>[0]),
                }))}
              />
            </div>
          </div>
        ) : (
          <div className="py-6 text-center text-dim">…</div>
        )}

        {sample ? (
          <div className="mt-5 rounded-xl border border-white/[0.07] bg-[#0F0F13] p-4">
            <div className="mb-2 text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
              {t('naming.preview')}
            </div>
            <SampleLine label={t('naming.exMovie')} value={sample.movie} />
            <SampleLine label={t('naming.exEpisode')} value={sample.episode} />
          </div>
        ) : null}

        <div className="mt-5 flex items-center gap-3">
          <button
            type="button"
            onClick={save}
            disabled={saving || !tpl}
            className="inline-flex items-center gap-2 rounded-xl bg-accent px-5 py-2.75 text-[14px] font-bold text-accent-ink hover:bg-accent-hover disabled:opacity-60"
          >
            {saving ? <IconLoader2 size={15} stroke={2.4} className="animate-spin" /> : null}
            {t('common.save')}
          </button>
          {saved ? (
            <span className="text-[13px] font-semibold text-[#46D08D]">{t('common.saved')}</span>
          ) : null}
        </div>
      </Card>

      <RenameSection />

      {tpl && openField ? (
        <NamingTokenModal
          fieldKey={openField}
          fieldLabel={t(
            FIELDS.find((f) => f.key === openField)?.labelKey as Parameters<typeof t>[0],
          )}
          value={tpl[openField]}
          onChange={setField(openField)}
          onClose={() => setOpenField(null)}
        />
      ) : null}
    </>
  );
}

function SampleLine({ label, value }: Readonly<{ label: string; value: string }>) {
  return (
    <div className="flex items-baseline gap-2 py-0.5">
      <span className="w-16 shrink-0 text-[11px] font-semibold text-dim">{label}</span>
      <code className="min-w-0 break-all font-mono text-[12.5px] text-[#86A8FF]">{value}</code>
    </div>
  );
}

function RenameSection() {
  const t = useT();
  const { client } = useAuth();
  const [plan, setPlan] = useState<OrganizePlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [result, setResult] = useState<string | null>(null);

  const preview = () => {
    setBusy(true);
    setResult(null);
    client
      .organizePreview()
      .then(setPlan)
      .catch((e) => setResult(apiErrorText(e, t('naming.previewFailed'))))
      .finally(() => setBusy(false));
  };
  const apply = () => {
    setConfirm(false);
    setBusy(true);
    client
      .organizeApply()
      .then((r) => {
        setResult(t('naming.applied', { moved: String(r.moved), failed: String(r.failed) }));
        setPlan(null);
      })
      .catch((e) => setResult(apiErrorText(e, t('naming.applyFailed'))))
      .finally(() => setBusy(false));
  };

  return (
    <Section
      title={t('naming.renameTitle')}
      right={
        <button
          type="button"
          onClick={preview}
          disabled={busy}
          className="inline-flex items-center gap-1.5 rounded-lg border border-white/12 bg-[#1A1A20] px-3.5 py-2 text-[13px] font-semibold text-white/80 hover:bg-[#222229] disabled:opacity-60"
        >
          {busy ? (
            <IconLoader2 size={14} stroke={2.4} className="animate-spin" />
          ) : (
            <IconWand size={14} stroke={2} />
          )}
          {t('naming.preview2')}
        </button>
      }
    >
      <p className="mb-3 text-[13.5px] leading-relaxed text-dim">{t('naming.renameHelp')}</p>

      {result ? (
        <div className="mb-3 rounded-lg border border-white/[0.08] bg-[#121216] px-4 py-2.5 text-[13px] font-semibold text-white/80">
          {result}
        </div>
      ) : null}

      {plan ? (
        <Card className="p-4">
          <div className="mb-3 text-[13px] font-semibold text-white/70">
            {t('naming.planSummary', {
              moves: String(plan.moves.length),
              matching: String(plan.matching),
              total: String(plan.totalFiles),
            })}
          </div>
          {plan.moves.length > 0 ? (
            <>
              <div className="max-h-80 overflow-y-auto rounded-xl border border-white/[0.07] bg-[#0F0F13]">
                {plan.moves.slice(0, 200).map((m) => (
                  <MoveRow key={`${m.from}`} from={m.from} to={m.to} />
                ))}
              </div>
              <button
                type="button"
                onClick={() => setConfirm(true)}
                disabled={busy}
                className="mt-4 inline-flex items-center gap-2 rounded-xl bg-accent px-5 py-2.75 text-[14px] font-bold text-accent-ink hover:bg-accent-hover disabled:opacity-60"
              >
                {t('naming.apply', { n: String(plan.moves.length) })}
              </button>
            </>
          ) : (
            <div className="py-4 text-center text-[13.5px] font-medium text-[#46D08D]">
              {t('naming.allMatch')}
            </div>
          )}
        </Card>
      ) : null}

      {confirm ? (
        <Modal title={t('naming.confirmTitle')} onClose={() => setConfirm(false)}>
          <p className="text-[13.5px] leading-relaxed text-white/75">{t('naming.confirmBody')}</p>
          <ModalActions
            onCancel={() => setConfirm(false)}
            cancelLabel={t('common.cancel')}
            onConfirm={apply}
            confirmLabel={t('naming.confirmApply')}
            busy={busy}
          />
        </Modal>
      ) : null}
    </Section>
  );
}

function MoveRow({ from, to }: Readonly<{ from: string; to: string }>): ReactNode {
  return (
    <div className="flex items-center gap-2 border-b border-white/[0.04] px-3 py-2 text-[11.5px] last:border-0">
      <code className="min-w-0 flex-1 truncate font-mono text-white/45" title={from}>
        {from}
      </code>
      <IconArrowRight size={13} stroke={2} className="shrink-0 text-white/30" />
      <code className="min-w-0 flex-1 truncate font-mono text-[#86A8FF]" title={to}>
        {to}
      </code>
    </div>
  );
}
