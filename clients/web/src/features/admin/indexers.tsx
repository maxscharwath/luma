// Admin "Indexeurs" page: the configured Torznab endpoints (Jackett /
// Prowlarr) as a card grid with enable toggles, live test (t=caps latency +
// TMDB id support) and an add/edit modal. Structure mirrors the libraries page.

import { apiErrorText, type IndexerTestResult, type IndexerView } from '@luma/core';
import { useT } from '@luma/ui';
import { IconAntenna, IconLoader2, IconPencil } from '@tabler/icons-react';
import { useState } from 'react';
import { IndexerModal } from '#web/features/admin/indexer-modals';
import { Denied, HeaderAction, PageHeader, useCap, usePoll } from '#web/features/admin/shell';
import { Card, Pill, Toggle } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';
import { EmptyState, TableSkeleton } from '#web/shared/ui';

type TestState = { busy?: boolean; result?: IndexerTestResult; error?: string };

export function IndexersPage() {
  const t = useT();
  const { client } = useAuth();
  const canManage = useCap('settings.manage');
  const [modal, setModal] = useState<{ open: boolean; indexer: IndexerView | null }>({
    open: false,
    indexer: null,
  });
  const [tests, setTests] = useState<Record<string, TestState>>({});

  const { data, reload } = usePoll(['admin', 'indexers'], () => client.adminIndexers(), 30000);

  if (!canManage) return <Denied />;
  const indexers = data?.indexers ?? [];

  const toggle = (ix: IndexerView, enabled: boolean) => {
    client
      .updateIndexer(ix.id, {
        name: null,
        url: null,
        apiKey: null,
        categories: null,
        enabled,
        priority: null,
      })
      .then(reload)
      .catch(() => reload());
  };

  const test = (ix: IndexerView) => {
    setTests((s) => ({ ...s, [ix.id]: { busy: true } }));
    client
      .testIndexer(ix.id)
      .then((result) => setTests((s) => ({ ...s, [ix.id]: { result } })))
      .catch((e) =>
        setTests((s) => ({ ...s, [ix.id]: { error: apiErrorText(e, t('indexers.testFailed')) } })),
      )
      .finally(reload);
  };

  return (
    <>
      <PageHeader
        title={t('admin.indexersTitle')}
        subtitle={t('admin.indexersSub')}
        action={
          <HeaderAction
            label={t('indexers.add')}
            onClick={() => setModal({ open: true, indexer: null })}
          />
        }
      />

      {data === null ? <TableSkeleton rows={5} /> : null}

      {indexers.length === 0 && data ? (
        <EmptyState
          icon={<IconAntenna size={32} stroke={1.5} />}
          title={t('indexers.emptyTitle')}
          hint={t('indexers.emptyBody')}
          action={
            <HeaderAction
              label={t('indexers.add')}
              onClick={() => setModal({ open: true, indexer: null })}
            />
          }
        />
      ) : null}

      <div className="mt-6 grid grid-cols-1 gap-4 xl:grid-cols-2">
        {indexers.map((ix) => (
          <IndexerCard
            key={ix.id}
            ix={ix}
            test={tests[ix.id]}
            onToggle={(v) => toggle(ix, v)}
            onTest={() => test(ix)}
            onEdit={() => setModal({ open: true, indexer: ix })}
          />
        ))}
      </div>

      {modal.open ? (
        <IndexerModal
          indexer={modal.indexer}
          onClose={() => setModal({ open: false, indexer: null })}
          onSaved={reload}
        />
      ) : null}
    </>
  );
}

function IndexerCard({
  ix,
  test,
  onToggle,
  onTest,
  onEdit,
}: Readonly<{
  ix: IndexerView;
  test?: TestState;
  onToggle: (v: boolean) => void;
  onTest: () => void;
  onEdit: () => void;
}>) {
  const t = useT();
  return (
    <Card className="p-5">
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 items-center gap-3.5">
          <span className="flex h-11 w-11 flex-[0_0_44px] items-center justify-center rounded-xl border border-border-strong bg-surface-2 text-accent">
            <IconAntenna size={20} stroke={1.8} />
          </span>
          <div className="min-w-0">
            <div className="flex items-center gap-2.5">
              <span className="truncate text-[15.5px] font-bold">{ix.name}</span>
              {!ix.enabled ? (
                <Pill color="rgba(244,243,240,.55)">{t('indexers.disabled')}</Pill>
              ) : null}
            </div>
            <div className="mt-0.5 truncate text-[12.5px] font-medium text-dim">{ix.url}</div>
          </div>
        </div>
        <Toggle on={ix.enabled} onChange={onToggle} />
      </div>

      <div className="mt-3.5 flex flex-wrap items-center gap-2 text-[12px] font-semibold text-white/55">
        <Pill color="#86A8FF">{t('indexers.cats', { cats: ix.categories.join(', ') })}</Pill>
        {ix.priority !== 0 ? (
          <Pill color="#C792EA">{t('indexers.prio', { prio: String(ix.priority) })}</Pill>
        ) : null}
        {ix.hasApiKey ? <Pill color="#46D08D">{t('indexers.keySet')}</Pill> : null}
      </div>

      <div className="mt-4 flex items-center justify-between gap-3 border-t border-white/[0.06] pt-3.5">
        <TestLine ix={ix} test={test} />
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onTest}
            disabled={test?.busy}
            className="inline-flex items-center gap-1.5 rounded-lg border border-white/12 bg-[#1A1A20] px-3 py-2 text-[12.5px] font-semibold text-white/80 hover:bg-[#222229] disabled:opacity-60"
          >
            {test?.busy ? <IconLoader2 size={13} stroke={2.4} className="animate-spin" /> : null}
            {t('indexers.test')}
          </button>
          <button
            type="button"
            onClick={onEdit}
            title={t('indexers.edit')}
            className="flex h-[34px] w-[34px] items-center justify-center rounded-lg border border-white/12 bg-[#1A1A20] text-white/70 hover:text-white"
          >
            <IconPencil size={14} stroke={2} />
          </button>
        </div>
      </div>
    </Card>
  );
}

function TestLine({ ix, test }: Readonly<{ ix: IndexerView; test?: TestState }>) {
  const t = useT();
  if (test?.busy) {
    return (
      <span className="text-[12.5px] font-semibold text-white/45">{t('indexers.testing')}</span>
    );
  }
  if (test?.error || test?.result?.error) {
    return (
      <span className="min-w-0 truncate text-[12.5px] font-semibold text-[#EF8091]">
        {test.error ?? test.result?.error}
      </span>
    );
  }
  if (test?.result) {
    return (
      <span className="text-[12.5px] font-semibold text-[#46D08D]">
        {t('indexers.testOk', {
          ms: String(test.result.latencyMs),
          server: test.result.serverTitle ?? 'Torznab',
        })}
        {test.result.supportsTmdb ? ` · ${t('indexers.tmdbOk')}` : ''}
      </span>
    );
  }
  if (ix.lastError) {
    return (
      <span className="min-w-0 truncate text-[12.5px] font-semibold text-[#EF8091]">
        {ix.lastError}
      </span>
    );
  }
  if (ix.lastOkAt) {
    return (
      <span className="text-[12.5px] font-medium text-white/45">
        {t('indexers.lastOk', { date: new Date(ix.lastOkAt).toLocaleString() })}
      </span>
    );
  }
  return (
    <span className="text-[12.5px] font-medium text-white/35">{t('indexers.neverTested')}</span>
  );
}
