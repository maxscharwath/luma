// Expandable per-job detail: the recent run history (left) and the selected
// run's log lines (right). Polls `/api/admin/jobs/:key` + `/api/admin/job-runs/
// :runId/logs`; while a run is active the short poll interval makes the logs
// feel live.

import type { JobLog, JobRun, MessageKey } from '@luma/core';
import { useT } from '@luma/ui';
import { useState } from 'react';
import { clock, dur, rel } from '#web/features/admin/jobs-format';
import { useAdmin, usePoll } from '#web/features/admin/shell';
import { C } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';

export function JobDetailPanel({ jobKey }: Readonly<{ jobKey: string }>) {
  const t = useT();
  const { client } = useAuth();
  const { tick } = useAdmin();
  const { data } = usePoll(() => client.adminJob(jobKey), 4000, [client, jobKey, tick]);
  const runs = data?.runs ?? [];

  const [selected, setSelected] = useState<string | null>(null);
  const runId = selected ?? runs[0]?.id ?? null;
  const { data: logsData } = usePoll(
    () => (runId ? client.jobRunLogs(runId) : Promise.resolve({ logs: [] as JobLog[] })),
    2500,
    [client, runId, tick],
  );
  const logs = logsData?.logs ?? [];

  return (
    <div className="grid gap-px border-t border-border bg-border md:grid-cols-[270px_1fr]">
      <div className="max-h-80 overflow-y-auto bg-surface-1 p-2.5">
        {runs.length === 0 ? (
          <div className="px-2 py-6 text-center text-[12.5px] text-dim">{t('jobs.noRuns')}</div>
        ) : (
          runs.map((r) => (
            <RunRow key={r.id} run={r} active={r.id === runId} onClick={() => setSelected(r.id)} />
          ))
        )}
      </div>
      <div className="max-h-80 overflow-y-auto bg-[#0B0B0D] p-3.5 font-mono text-[12px] leading-relaxed">
        {logs.length === 0 ? (
          <div className="text-[12.5px] text-dim">{t('jobs.noLogs')}</div>
        ) : (
          logs.map((l, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: append-only log lines, never reordered
            <LogLine key={i} log={l} />
          ))
        )}
      </div>
    </div>
  );
}

function RunRow({
  run,
  active,
  onClick,
}: Readonly<{ run: JobRun; active: boolean; onClick: () => void }>) {
  const t = useT();
  return (
    <button
      type="button"
      onClick={onClick}
      className={`mb-1 flex w-full items-center gap-2.5 rounded-[9px] px-2.5 py-2 text-left transition-colors ${
        active ? 'bg-white/6' : 'hover:bg-white/3'
      }`}
    >
      <span
        className="h-2 w-2 shrink-0 rounded-full"
        style={{ background: statusColor(run.status) }}
      />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[12.5px] font-semibold text-text">
          {t(`jobs.status.${run.status}` as MessageKey)}
          <span className="ml-1.5 font-normal text-text/45">
            · {t(`jobs.trigger.${run.trigger}` as MessageKey)}
          </span>
        </span>
        <span className="block text-[11px] text-dim">
          {rel(run.startedAt)}
          {run.durationMs != null ? ` · ${dur(run.durationMs)}` : ''}
        </span>
      </span>
    </button>
  );
}

function logColor(level: string): string {
  if (level === 'error') return C.red;
  if (level === 'warn') return C.accent;
  return '#A8AEB8';
}

function LogLine({ log }: Readonly<{ log: JobLog }>) {
  const color = logColor(log.level);
  return (
    <div className="flex gap-2.5">
      <span className="shrink-0 text-text/35">{clock(log.ts)}</span>
      <span className="whitespace-pre-wrap break-words" style={{ color }}>
        {log.message}
      </span>
    </div>
  );
}

function statusColor(status: string): string {
  if (status === 'success') return C.green;
  if (status === 'failed') return C.red;
  if (status === 'running') return C.accent;
  return '#9AA0AA';
}
