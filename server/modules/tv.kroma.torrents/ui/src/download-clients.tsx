// The "Clients de téléchargement" section of the downloads page: one card per
// engine (embedded / Transmission / qBittorrent) with enable toggle, live
// connection test and the add/edit modal.

import {
  AddEngineModal,
  apiErrorText,
  Card,
  type ClientTestResult,
  type DownloadClientView,
  EmptyState,
  Pill,
  Section,
  TableSkeleton,
  Toggle,
  useAdminKit,
  useEnabledEngines,
  usePoll,
  useT,
} from '@kroma/module-sdk';
import { IconCpu, IconLoader2, IconPencil, IconPlus, IconServer } from '@tabler/icons-react';
import { useState } from 'react';
import { DownloadClientModal } from './download-client-modals';

type TestState = { busy?: boolean; result?: ClientTestResult; error?: string };

export function DownloadClientsSection() {
  const t = useT();
  const { client: api } = useAdminKit();
  const engines = useEnabledEngines('download-client');
  const [tests, setTests] = useState<Record<string, TestState>>({});
  const { data, reload } = usePoll(
    ['admin', 'downloadClients'],
    () => api.adminDownloadClients(),
    30000,
  );
  const clients = data?.clients ?? [];

  const openAdd = async () => {
    const changed = await AddEngineModal.call({
      engines,
      title: t('dlclients.addTitle'),
      onSubmit: async (kind, v) => {
        await api.createDownloadClient({
          kind,
          name: v.name ?? null,
          url: v.url ?? null,
          username: v.username ?? null,
          password: v.password ?? null,
          enabled: true,
          priority: null,
        });
      },
    });
    if (changed) reload();
  };
  const openEdit = async (c: DownloadClientView) => {
    if (await DownloadClientModal.call({ client: c })) reload();
  };

  const toggle = (c: DownloadClientView, enabled: boolean) => {
    api
      .updateDownloadClient(c.id, {
        kind: null,
        name: null,
        url: null,
        username: null,
        password: null,
        enabled,
        priority: null,
      })
      .then(reload)
      .catch(() => reload());
  };
  const test = (c: DownloadClientView) => {
    setTests((s) => ({ ...s, [c.id]: { busy: true } }));
    api
      .testDownloadClient(c.id)
      .then((result) => setTests((s) => ({ ...s, [c.id]: { result } })))
      .catch((e) =>
        setTests((s) => ({ ...s, [c.id]: { error: apiErrorText(e, t('dlclients.testFailed')) } })),
      );
  };

  // One button reused by the section header and the empty state (only when an
  // external download-client engine is enabled).
  const addButton =
    engines.length > 0 ? (
      <button
        type="button"
        onClick={() => void openAdd()}
        className="inline-flex items-center gap-1.5 rounded-lg border border-white/12 bg-[#1A1A20] px-3 py-2 text-[12.5px] font-semibold text-white/80 hover:bg-[#222229]"
      >
        <IconPlus size={14} stroke={2.4} />
        {t('dlclients.add')}
      </button>
    ) : null;

  return (
    <Section title={t('dlclients.sectionTitle')} right={addButton}>
      {data === null ? <TableSkeleton rows={3} /> : null}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {clients.map((c) => (
          <Card key={c.id} className="p-4.5">
            <div className="flex items-start justify-between gap-4">
              <div className="flex min-w-0 items-center gap-3">
                <span className="flex h-10 w-10 flex-[0_0_40px] items-center justify-center rounded-xl border border-border-strong bg-surface-2 text-accent">
                  {c.builtin ? (
                    <IconCpu size={18} stroke={1.8} />
                  ) : (
                    <IconServer size={18} stroke={1.8} />
                  )}
                </span>
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[14.5px] font-bold">{c.name}</span>
                    <Pill color="#86A8FF">{c.builtin ? t('dlclients.embedded') : c.kind}</Pill>
                  </div>
                  <div className="mt-0.5 truncate text-[12px] font-medium text-dim">
                    {c.builtin ? t('dlclients.embeddedSub') : c.url}
                  </div>
                </div>
              </div>
              <Toggle on={c.enabled} onChange={(v) => toggle(c, v)} />
            </div>
            <div className="mt-3.5 flex items-center justify-between gap-3 border-t border-white/6 pt-3">
              <TestLine test={tests[c.id]} />
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => test(c)}
                  disabled={tests[c.id]?.busy}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-white/12 bg-[#1A1A20] px-3 py-1.5 text-[12px] font-semibold text-white/80 hover:bg-[#222229] disabled:opacity-60"
                >
                  {tests[c.id]?.busy ? (
                    <IconLoader2 size={12} stroke={2.4} className="animate-spin" />
                  ) : null}
                  {t('dlclients.test')}
                </button>
                {!c.builtin ? (
                  <button
                    type="button"
                    onClick={() => void openEdit(c)}
                    title={t('dlclients.edit')}
                    className="flex h-[30px] w-[30px] items-center justify-center rounded-lg border border-white/12 bg-[#1A1A20] text-white/70 hover:text-white"
                  >
                    <IconPencil size={13} stroke={2} />
                  </button>
                ) : null}
              </div>
            </div>
          </Card>
        ))}
      </div>
      {data && clients.length === 0 ? (
        <EmptyState
          icon={<IconServer size={32} stroke={1.5} />}
          title={t('dlclients.empty')}
          action={addButton ?? undefined}
        />
      ) : null}

      <DownloadClientModal />
    </Section>
  );
}

function TestLine({ test }: Readonly<{ test?: TestState }>) {
  const t = useT();
  if (test?.busy) {
    return (
      <span className="text-[12px] font-semibold text-white/45">{t('dlclients.testing')}</span>
    );
  }
  if (test?.error || test?.result?.error) {
    return (
      <span className="min-w-0 truncate text-[12px] font-semibold text-[#EF8091]">
        {test.error ?? test.result?.error}
      </span>
    );
  }
  if (test?.result?.ok) {
    return <span className="text-[12px] font-semibold text-[#46D08D]">{test.result.version}</span>;
  }
  return <span className="text-[12px] font-medium text-white/30">{t('dlclients.notTested')}</span>;
}
