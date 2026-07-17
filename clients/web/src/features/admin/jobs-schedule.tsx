// The "edit schedule" modal for a background job: a cron expression input with
// common presets, a "manual only" option, and reset-to-default. Posts to
// `PATCH /api/admin/jobs/:key`; the server validates the cron and 400s on a bad
// expression, surfaced inline here.

import { type JobInfo, KromaApiError } from '@kroma/core';
import { useT } from '@kroma/ui';
import { useState } from 'react';
import { useAsyncAction } from '#web/features/admin/shell';
import { Field, Modal, ModalActions, TextInput } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';

const PRESETS: { label: string; expr: string }[] = [
  { label: '@hourly', expr: '0 * * * *' },
  { label: '04:00', expr: '0 4 * * *' },
  { label: '05:00', expr: '0 5 * * *' },
  { label: 'Sun 03:00', expr: '0 3 * * 0' },
  { label: '1st 03:00', expr: '0 3 1 * *' },
];

export function ScheduleModal({
  job,
  onClose,
  onSaved,
}: Readonly<{ job: JobInfo; onClose: () => void; onSaved: () => void }>) {
  const t = useT();
  const { client } = useAuth();
  const [value, setValue] = useState(job.schedule ?? '');
  const { busy, error, run } = useAsyncAction();

  const save = () =>
    run(
      async () => {
        await client.updateJob(job.key, { schedule: value.trim() || null });
        onSaved();
      },
      (e) =>
        e instanceof KromaApiError && e.status === 400
          ? t('jobs.cronInvalid')
          : t('jobs.saveFailed'),
    );

  return (
    <Modal title={t('jobs.editSchedule')} onClose={onClose}>
      <Field label={t('jobs.cronExpr')}>
        <TextInput
          value={value}
          onChange={setValue}
          placeholder="0 4 * * *"
          className="w-full font-mono"
        />
      </Field>

      <div className="mb-3 flex flex-wrap gap-2">
        {PRESETS.map((p) => (
          <button
            key={p.expr}
            type="button"
            onClick={() => setValue(p.expr)}
            className="rounded-[7px] border border-border-strong bg-surface-2 px-2.5 py-1 font-mono text-[11.5px] font-semibold text-text/75 hover:border-accent/50"
          >
            {p.label}
          </button>
        ))}
        <button
          type="button"
          onClick={() => setValue('')}
          className="rounded-[7px] border border-border-strong bg-surface-2 px-2.5 py-1 text-[11.5px] font-semibold text-text/75 hover:border-accent/50"
        >
          {t('jobs.manual')}
        </button>
      </div>

      <p className="mb-1 text-[12px] leading-relaxed text-dim">{t('jobs.cronHint')}</p>
      {job.defaultSchedule && job.defaultSchedule !== value ? (
        <button
          type="button"
          onClick={() => setValue(job.defaultSchedule ?? '')}
          className="text-[12px] font-semibold text-accent"
        >
          {t('jobs.resetDefault')} ({job.defaultSchedule})
        </button>
      ) : null}

      {error ? (
        <div className="mt-3 text-[12.5px] font-semibold text-[#E8536A]">{error}</div>
      ) : null}

      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={save}
        confirmLabel={busy ? t('jobs.saving') : t('common.save')}
        busy={busy}
      />
    </Modal>
  );
}
