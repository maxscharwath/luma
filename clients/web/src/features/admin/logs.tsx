// Admin "Journaux" console: the server's recent log lines (core + module
// sidecars) from the in-memory ring over `/api/admin/logs`, with level/source/
// text filters and a follow-tail toggle. Polls; the ring is the source of
// truth so a page load shows history, not just what streams in afterwards.

import type { LogEntry, MessageKey } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconSearch, IconTerminal2 } from '@tabler/icons-react';
import { useEffect, useRef, useState } from 'react';
import { PageHeader, usePoll } from '#web/features/admin/shell';
import { Card, SegmentedControl, Select, Toggle } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';
import { EmptyState, TableSkeleton } from '#web/shared/ui';
import { InputGroup, InputGroupAddon, InputGroupInput } from '#web/shared/ui/input-group';

type LevelFilter = 'all' | 'info' | 'warn' | 'error';

const LEVELS: { value: LevelFilter; labelKey: MessageKey }[] = [
  { value: 'all', labelKey: 'logs.levelAll' },
  { value: 'info', labelKey: 'logs.levelInfo' },
  { value: 'warn', labelKey: 'logs.levelWarn' },
  { value: 'error', labelKey: 'logs.levelError' },
];

export function LogsPage() {
  const t = useT();
  const { client } = useAuth();
  const [level, setLevel] = useState<LevelFilter>('all');
  const [source, setSource] = useState('all');
  const [qInput, setQInput] = useState('');
  const [q, setQ] = useState('');
  const [follow, setFollow] = useState(true);

  // Debounce the search box so typing doesn't fire a request per keystroke.
  useEffect(() => {
    const id = setTimeout(() => setQ(qInput.trim()), 350);
    return () => clearTimeout(id);
  }, [qInput]);

  const { data } = usePoll(
    ['admin', 'logs', level, source, q],
    () =>
      client.adminLogs({
        level: level === 'all' ? undefined : level,
        source: source === 'all' ? undefined : source,
        q: q || undefined,
        limit: 1000,
      }),
    3000,
  );

  // Follow the tail: pin the viewport to the newest line on every refresh.
  const scroller = useRef<HTMLDivElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll on new data
  useEffect(() => {
    if (follow && scroller.current) {
      scroller.current.scrollTop = scroller.current.scrollHeight;
    }
  }, [data, follow]);

  const entries = data?.entries ?? [];
  const sources = ['all', ...(data?.sources ?? [])];
  const sourceLabel = (s: string) => {
    if (s === 'all') return t('logs.allSources');
    return s === 'core' ? t('logs.sourceCore') : s;
  };

  return (
    <>
      <PageHeader title={t('admin.logsTitle')} subtitle={t('admin.logsSub')} realtime />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <SegmentedControl
          value={level}
          options={LEVELS.map((l) => ({ value: l.value, label: t(l.labelKey) }))}
          onChange={setLevel}
        />
        <Select
          value={sourceLabel(source)}
          options={sources.map(sourceLabel)}
          onChange={(label) => setSource(sources.find((s) => sourceLabel(s) === label) ?? 'all')}
        />
        <InputGroup className="h-9 w-64">
          <InputGroupAddon>
            <IconSearch size={15} />
          </InputGroupAddon>
          <InputGroupInput
            value={qInput}
            onChange={(e) => setQInput(e.target.value)}
            placeholder={t('logs.searchPlaceholder')}
            className="text-[13px]"
          />
        </InputGroup>
        <div className="ml-auto flex items-center gap-2 text-[13px] font-semibold text-muted">
          <span>{t('logs.follow')}</span>
          <Toggle on={follow} onChange={setFollow} />
        </div>
      </div>
      {data === null ? <TableSkeleton rows={10} /> : null}
      {data && entries.length === 0 ? (
        <EmptyState icon={<IconTerminal2 size={32} stroke={1.5} />} title={t('logs.empty')} />
      ) : null}
      {entries.length > 0 ? (
        <Card className="p-0">
          <div ref={scroller} className="max-h-[70vh] overflow-y-auto px-4 py-3">
            {entries.map((e) => (
              <LogLine key={`${e.ts}-${e.source}-${e.message}`} entry={e} />
            ))}
          </div>
        </Card>
      ) : null}
    </>
  );
}

const LEVEL_TONE: Record<string, string> = {
  error: 'bg-red-500/15 text-red-400',
  warn: 'bg-amber-500/15 text-amber-400',
  info: 'bg-white/6 text-muted',
  debug: 'bg-white/4 text-dim',
  trace: 'bg-white/4 text-dim',
};

function LogLine({ entry }: Readonly<{ entry: LogEntry }>) {
  const time = new Date(entry.ts).toLocaleTimeString(undefined, { hour12: false });
  return (
    <div className="flex items-baseline gap-2.5 border-b border-white/4 py-1 font-mono text-[12px] leading-relaxed last:border-b-0">
      <span className="shrink-0 tabular-nums text-dim">{time}</span>
      <span
        className={`w-13 shrink-0 rounded px-1.5 text-center text-[10px] font-bold uppercase ${LEVEL_TONE[entry.level] ?? LEVEL_TONE.info}`}
      >
        {entry.level}
      </span>
      {entry.source !== 'core' ? (
        <span className="shrink-0 rounded bg-accent-soft px-1.5 text-[10px] font-semibold text-accent">
          {entry.source.replace(/^dev\.kroma\./, '')}
        </span>
      ) : null}
      <span className="min-w-0 wrap-break-word text-text/85">
        {entry.target ? <span className="text-dim">{entry.target}: </span> : null}
        {entry.message}
      </span>
    </div>
  );
}
