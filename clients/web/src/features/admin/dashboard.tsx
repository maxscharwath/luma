import type { MetricsSnapshot, PlaybackSession, TopUser } from '@luma/core';
import { useT } from '@luma/ui';
import { useMemo, useState } from 'react';
import { HistoryBars, MetricsChart } from '#web/features/admin/charts';
import {
  NowPlayingCard,
  StopStreamModal,
} from '#web/features/admin/dashboard-now-playing';
import { PageHeader, useAdmin, usePoll } from '#web/features/admin/shell';
import { Avatar, C, Card, FilterLabel, Section } from '#web/features/admin/ui';
import { decimal, formatDuration, formatMbps } from '#web/shared/lib/adminFormat';
import { useAuth } from '#web/shared/lib/auth';

export function DashboardScreen() {
  const t = useT();
  const { client } = useAuth();
  const { serverInfo, tick } = useAdmin();

  const { data: sessionsData, reload: reloadSessions } = usePoll(
    () => client.adminSessions(),
    3000,
    [client, tick],
  );
  // The server samples every 3s; polling faster only redraws identical charts.
  const { data: metrics } = usePoll(() => client.adminMetrics(), 5000, [client]);
  const { data: top } = usePoll(() => client.topUsers(7), 30000, [client, tick]);
  const { data: history } = usePoll(() => client.playHistory(28), 60000, [client, tick]);
  // Avatars for the now-playing cards come from the authenticated admin roster,
  // not the public `/users` picker list (which the `publicUserList` setting can
  // hide). Needs `users.manage`; without it the map stays empty (cards fall back
  // to name-based avatars), which is harmless.
  const { data: usersData } = usePoll(() => client.adminUsers(), 60000, [client, tick]);

  const [stopTarget, setStopTarget] = useState<PlaybackSession | null>(null);
  const sessions = sessionsData?.sessions ?? [];
  // Map each streaming user to their uploaded avatar (sessions carry only a name).
  const avatarByUser = useMemo(() => {
    const m = new Map<string, string | null>();
    for (const u of usersData?.users ?? []) m.set(u.id, u.avatarUrl ?? null);
    return m;
  }, [usersData]);

  return (
    <>
      <PageHeader title={serverInfo?.name ?? 'LUMA'} suffix={t('admin.dashboardSuffix')} realtime />

      <Section title={t('admin.nowPlaying')}>
        {sessions.length === 0 ? (
          <Card className="px-6 py-10 text-center text-[14px] text-dim">
            {t('admin.noPlayback')}
          </Card>
        ) : (
          <div className="flex flex-col gap-3.5">
            {sessions.map((s) => (
              <NowPlayingCard
                key={s.id}
                s={s}
                avatarUrl={s.userId ? avatarByUser.get(s.userId) : null}
                onStop={() => setStopTarget(s)}
              />
            ))}
          </div>
        )}
      </Section>

      {stopTarget ? (
        <StopStreamModal
          session={stopTarget}
          onClose={() => setStopTarget(null)}
          onStopped={() => {
            setStopTarget(null);
            reloadSessions();
          }}
        />
      ) : null}

      <BandwidthSection metrics={metrics} />
      <CpuSection metrics={metrics} />
      <RamSection metrics={metrics} />

      <Section
        title={t('admin.topUsers')}
        right={<FilterLabel>{t('admin.last7days')}</FilterLabel>}
      >
        {top && top.users.length > 0 ? (
          <div className="grid grid-cols-3 gap-4">
            {top.users.slice(0, 3).map((u) => (
              <TopUserCard key={u.username} u={u} />
            ))}
          </div>
        ) : (
          <Card className="px-6 py-8 text-center text-[14px] text-dim">{t('admin.noHistory')}</Card>
        )}
      </Section>

      <Section
        title={t('admin.playHistory')}
        right={<FilterLabel>{t('admin.last30days')}</FilterLabel>}
      >
        {history ? <HistoryBars buckets={history.buckets} /> : null}
      </Section>
    </>
  );
}

// ----- metric chart sections --------------------------------------------------

const avg = (a: number[]) => (a.length ? a.reduce((x, y) => x + y, 0) / a.length : 0);

const pct = (v: number) => `${Math.round(v)} %`;

function BandwidthSection({ metrics }: Readonly<{ metrics: MetricsSnapshot | null }>) {
  const t = useT();
  const local = metrics?.series.bwLocal ?? [];
  const remote = metrics?.series.bwRemote ?? [];
  const max = Math.max(1, ...local, ...remote);
  return (
    <Section title={t('admin.bandwidth')} right={<FilterLabel>{t('admin.realtime')}</FilterLabel>}>
      <MetricsChart
        max={max}
        formatValue={formatMbps}
        series={[
          { label: t('admin.legendRemote'), data: remote, color: C.blue },
          { label: t('admin.legendLocal'), data: local, color: C.accent, fill: true },
        ]}
        legend={[
          { label: t('admin.legendRemote'), color: C.blue },
          { label: t('admin.legendLocal'), color: C.accent },
        ]}
        footer={t('admin.bwAverages', {
          remote: formatMbps(avg(remote)),
          local: formatMbps(avg(local)),
        })}
      />
    </Section>
  );
}

function CpuSection({ metrics }: Readonly<{ metrics: MetricsSnapshot | null }>) {
  const t = useT();
  const luma = metrics?.series.cpuLuma ?? [];
  const sys = metrics?.series.cpuSystem ?? [];
  return (
    <Section title={t('admin.cpu')} right={<FilterLabel>{t('admin.realtime')}</FilterLabel>}>
      <MetricsChart
        max={100}
        formatValue={pct}
        series={[
          { label: t('admin.legendSystem'), data: sys, color: C.cpuRed },
          { label: t('admin.legendLumaServer'), data: luma, color: C.green },
        ]}
        legend={[
          { label: t('admin.legendLumaServer'), color: C.green },
          { label: t('admin.legendSystem'), color: C.cpuRed },
        ]}
        footer={t('admin.cpuAverages', { luma: decimal(avg(luma), 1), sys: decimal(avg(sys), 1) })}
      />
    </Section>
  );
}

function RamSection({ metrics }: Readonly<{ metrics: MetricsSnapshot | null }>) {
  const t = useT();
  const luma = metrics?.series.ramLuma ?? [];
  const sys = metrics?.series.ramSystem ?? [];
  return (
    <Section title={t('admin.ram')} right={<FilterLabel>{t('admin.realtime')}</FilterLabel>}>
      <MetricsChart
        max={100}
        formatValue={pct}
        series={[
          { label: t('admin.legendSystem'), data: sys, color: C.purple },
          { label: t('admin.legendLumaServer'), data: luma, color: C.green },
        ]}
        legend={[
          { label: t('admin.legendLumaServer'), color: C.green },
          { label: t('admin.legendSystem'), color: C.purple },
        ]}
        footer={t('admin.ramAverages', { luma: decimal(avg(luma), 2), sys: decimal(avg(sys), 2) })}
      />
    </Section>
  );
}

// ----- top users --------------------------------------------------------------

function TopUserCard({ u }: Readonly<{ u: TopUser }>) {
  const t = useT();
  const rows = [
    { label: t('admin.films'), val: formatDuration(u.filmsMs), on: u.filmsMs >= u.tvMs },
    { label: t('admin.tv'), val: formatDuration(u.tvMs), on: u.tvMs > u.filmsMs },
  ];
  return (
    <Card className="overflow-hidden">
      <div className="flex items-center gap-3.5 px-5 py-4.5">
        <Avatar name={u.username} size={48} />
        <div>
          <div className="font-display text-[16px] font-bold">
            {u.plays} {u.plays > 1 ? t('admin.plays') : t('admin.play')}
          </div>
          <div className="text-[13px] font-medium text-text/55">{formatDuration(u.watchedMs)}</div>
        </div>
      </div>
      <div className="border-y border-white/5 bg-surface-2 px-5 py-2.75 text-[15px] font-bold">
        {u.username}
      </div>
      <div>
        {rows.map((r) => (
          <div
            key={r.label}
            className="flex items-center justify-between border-b border-white/4 px-5 py-2.75"
            style={{ background: r.on ? 'rgba(242,180,66,.16)' : 'transparent' }}
          >
            <span
              className="text-[13.5px] font-semibold"
              style={{ color: r.on ? C.accent : 'var(--luma-text-muted)' }}
            >
              {r.label}
            </span>
            <span
              className="text-[13.5px] font-semibold tabular-nums"
              style={{ color: r.on ? C.accent : 'var(--luma-text-muted)' }}
            >
              {r.val}
            </span>
          </div>
        ))}
      </div>
    </Card>
  );
}
