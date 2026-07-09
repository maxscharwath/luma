import type { Volume } from '@luma/core';
import { useT } from '@luma/ui';
import { IconDatabase } from '@tabler/icons-react';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { PageHeader, usePoll } from '#web/features/admin/shell';
import { C, Card, ProgressBar, Section, Select, StatCard } from '#web/features/admin/ui';
import { formatBytes } from '#web/shared/lib/adminFormat';
import { useAuth } from '#web/shared/lib/auth';

export const Route = createFileRoute('/admin/storage')({
  component: StoragePage,
});

function StoragePage() {
  const t = useT();
  const { client } = useAuth();
  const { data, reload } = usePoll(['admin', 'storage'], () => client.adminStorage(), 10000);
  const [clearing, setClearing] = useState(false);
  const [resetting, setResetting] = useState(false);

  const pctUsed = data?.totalBytes ? Math.round((data.usedBytes / data.totalBytes) * 100) : 0;
  const cache = data?.cache;
  const enriched = (cache?.enrichedItems ?? 0) + (cache?.enrichedShows ?? 0);

  async function clearCache() {
    setClearing(true);
    try {
      await client.clearCache();
      reload();
    } finally {
      setClearing(false);
    }
  }

  async function resetMetadata() {
    if (!confirm(t('admin.resetMetadataConfirm'))) return;
    setResetting(true);
    try {
      await client.resetMetadata();
      reload();
    } finally {
      setResetting(false);
    }
  }

  return (
    <>
      <PageHeader title={t('admin.storageTitle')} subtitle={t('admin.storageSub')} />

      <div className="mt-6 grid grid-cols-3 gap-4">
        <StatCard label={t('admin.totalCapacity')} value={formatBytes(data?.totalBytes ?? 0)} />
        <StatCard
          label={t('admin.used')}
          value={formatBytes(data?.usedBytes ?? 0)}
          unit={`${pctUsed}%`}
          color={C.accent}
        />
        <StatCard
          label={t('admin.available')}
          value={formatBytes(data?.availableBytes ?? 0)}
          color={C.green}
        />
      </div>

      <Section title={t('admin.volumes')}>
        <div className="flex flex-col gap-3.5">
          {(data?.volumes ?? []).map((v) => (
            <VolumeCard key={v.mount} v={v} />
          ))}
          {data && data.volumes.length === 0 ? (
            <Card className="px-6 py-8 text-center text-[14px] text-dim">
              {t('admin.noVolumes')}
            </Card>
          ) : null}
        </div>
      </Section>

      <Section title={t('admin.cacheContent')}>
        <div className="grid grid-cols-4 gap-4">
          <StatCard
            label={t('admin.transcodeCacheSize')}
            value={formatBytes(cache?.transcodeBytes ?? 0)}
            unit={t('admin.transcodeCacheBudget', { limit: cache?.transcodeLimit ?? '20 Go' })}
            color={C.accent}
          />
          <StatCard
            label={t('admin.cachedImages')}
            value={(cache?.imagesCount ?? 0).toLocaleString()}
            unit={formatBytes(cache?.imagesBytes ?? 0)}
            color={C.accent}
          />
          <StatCard
            label={t('admin.enrichedTitles')}
            value={enriched.toLocaleString()}
            unit={t('admin.enrichedBreakdown', {
              movies: cache?.enrichedItems ?? 0,
              shows: cache?.enrichedShows ?? 0,
            })}
            color={C.green}
          />
          <StatCard
            label={t('admin.cacheEmbeddings')}
            value={(cache?.embeddings ?? 0).toLocaleString()}
          />
        </div>
      </Section>

      <Section title={t('admin.cacheMaintenance')}>
        <Card className="overflow-hidden">
          <MaintRow
            title={t('admin.transcodeCacheFolder')}
            desc={t('admin.transcodeCacheFolderDesc')}
            right={
              <span className="rounded-[9px] border border-border-strong bg-surface-2 px-3 py-2 text-[13px] font-semibold text-text">
                {data?.cache.dir ?? '-'}
              </span>
            }
          />
          <MaintRow
            title={t('admin.cacheLimit')}
            desc={t('admin.cacheLimitDesc')}
            right={
              <Select
                value={data?.cache.limit ?? '80 Go'}
                options={['40 Go', '80 Go', '120 Go', '256 Go', t('opt.unlimited')]}
                onChange={(v) => client.updateSettings({ cacheLimit: v }).then(reload)}
              />
            }
          />
          <MaintRow
            title={t('admin.transcodeCacheLimit')}
            desc={t('admin.transcodeCacheLimitDesc')}
            right={
              <Select
                value={data?.cache.transcodeLimit ?? '20 Go'}
                options={['10 Go', '20 Go', '50 Go', '100 Go', t('opt.unlimited')]}
                onChange={(v) => client.updateSettings({ transcodeCacheLimit: v }).then(reload)}
              />
            }
          />
          <MaintRow
            title={t('admin.clearCache')}
            desc={t('admin.clearCacheDesc', { size: formatBytes(data?.cache.bytes ?? 0) })}
            right={
              <button
                type="button"
                onClick={() => void clearCache()}
                disabled={clearing}
                className="rounded-[9px] border border-[#E8536A]/25 bg-[#E8536A]/10 px-3.75 py-2.25 text-[13px] font-semibold text-[#E8536A] disabled:opacity-50"
              >
                {clearing ? t('admin.clearing') : t('admin.clearNow')}
              </button>
            }
          />
          <MaintRow
            title={t('admin.resetMetadata')}
            desc={t('admin.resetMetadataDesc')}
            border={false}
            right={
              <button
                type="button"
                onClick={() => void resetMetadata()}
                disabled={resetting}
                className="rounded-[9px] border border-[#E8536A]/25 bg-[#E8536A]/10 px-3.75 py-2.25 text-[13px] font-semibold text-[#E8536A] disabled:opacity-50"
              >
                {resetting ? t('admin.resetting') : t('admin.resetMetadataBtn')}
              </button>
            }
          />
        </Card>
      </Section>
    </>
  );
}

function VolumeCard({ v }: Readonly<{ v: Volume }>) {
  const t = useT();
  const pct = v.totalBytes ? Math.round((v.usedBytes / v.totalBytes) * 100) : 0;
  const barColor = pct >= 80 ? C.red : C.accent;
  return (
    <Card className="px-5.5 py-4.5">
      <div className="mb-3 flex items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-3.5">
          <span
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[11px]"
            style={{ background: 'rgba(244,182,66,.16)', color: C.accent }}
          >
            <IconDatabase size={20} stroke={1.8} />
          </span>
          <div className="min-w-0">
            <div className="font-display text-[16px] font-bold">{v.name || v.mount}</div>
            <div className="truncate text-[12.5px] font-semibold text-text/45">
              {v.mount} · {v.fs}
            </div>
          </div>
        </div>
        <div className="shrink-0 text-right">
          <div className="text-[15px] font-bold tabular-nums">
            {formatBytes(v.usedBytes)}{' '}
            <span className="font-medium text-text/40">/ {formatBytes(v.totalBytes)}</span>
          </div>
          <div className="text-[12px] font-semibold" style={{ color: barColor }}>
            {t('admin.pctUsed', { pct })}
          </div>
        </div>
      </div>
      <ProgressBar pct={pct} color={barColor} height={9} />
    </Card>
  );
}

function MaintRow({
  title,
  desc,
  right,
  border = true,
}: Readonly<{
  title: string;
  desc: string;
  right: React.ReactNode;
  border?: boolean;
}>) {
  return (
    <div
      className={`flex items-center justify-between gap-4.5 px-5.5 py-4 ${border ? 'border-b border-border' : ''}`}
    >
      <div>
        <div className="text-[14.5px] font-bold">{title}</div>
        <div className="mt-0.75 text-[12.5px] text-dim">{desc}</div>
      </div>
      <div className="shrink-0">{right}</div>
    </div>
  );
}
