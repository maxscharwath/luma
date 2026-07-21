import { OptionSelect } from '@kroma/admin-kit';
import type { MetricsSnapshot, PlaybackSession, TopUser } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconPlayerPlay, IconUsers } from '@tabler/icons-react';
import { useMemo, useState } from 'react';
import { HistoryBars, MetricsChart } from '#web/features/admin/charts';
import { NowPlayingCard, StopStreamModal } from '#web/features/admin/dashboard-now-playing';
import { PageHeader, useAdmin, usePoll } from '#web/features/admin/shell';
import { Avatar, C, Card, FilterLabel, Section } from '#web/features/admin/ui';
import { decimal, formatDuration, formatMbps } from '#web/shared/lib/adminFormat';
import { useAuth } from '#web/shared/lib/auth';
import { EmptyState } from '#web/shared/ui';

/** A range picker for the day-scoped stats sections (Top users, Play history),
 * bound to the `?days=` the backend already accepts. Replaces the old static,
 * chevroned caption that looked like a dropdown but did nothing. */
function RangeSelect({
  value,
  onChange,
  options,
  ariaLabel,
}: Readonly<{
  value: number;
  onChange: (days: number) => void;
  options: number[];
  ariaLabel: string;
}>) {
  const t = useT();
  return (
    <OptionSelect
      ariaLabel={ariaLabel}
      value={String(value)}
      onChange={(v) => onChange(Number(v))}
      options={options.map((d) => ({
        value: String(d),
        label: t('admin.lastNdays', { count: d }),
      }))}
    />
  );
}

/** Seconds between server metric samples, from the snapshot (falls back to 3s). */
const sampleSec = (metrics: MetricsSnapshot | null) => (metrics?.sampleIntervalMs ?? 3000) / 1000;

export function DashboardScreen() {
  const t = useT();
  const { client } = useAuth();
  const { serverInfo } = useAdmin();

  // Day-scoped ranges for the two analytics sections (the backend clamps them).
  const [topDays, setTopDays] = useState(7);
  const [historyDays, setHistoryDays] = useState(30);

  const { data: sessionsData, reload: reloadSessions } = usePoll(
    ['admin', 'sessions'],
    () => client.adminSessions(),
    3000,
  );
  // The server samples every 3s; polling faster only redraws identical charts.
  const { data: metrics } = usePoll(['admin', 'metrics'], () => client.adminMetrics(), 5000);
  // The range is in the poll key, so changing it refetches immediately.
  const { data: top } = usePoll(
    ['admin', 'topUsers', topDays],
    () => client.topUsers(topDays),
    30000,
  );
  const { data: history } = usePoll(
    ['admin', 'playHistory', historyDays],
    () => client.playHistory(historyDays),
    60000,
  );
  // Avatars for the now-playing cards come from the authenticated admin roster,
  // not the public `/users` picker list (which the `publicUserList` setting can
  // hide). Needs `users.manage`; without it the map stays empty (cards fall back
  // to name-based avatars), which is harmless.
  const { data: usersData } = usePoll(['admin', 'users'], () => client.adminUsers(), 60000);

  const sessions = sessionsData?.sessions ?? [];
  // Open the stop-stream confirmation imperatively; it resolves `true` once the
  // session was terminated, so we refresh the live list.
  const askStop = async (session: PlaybackSession) => {
    if (await StopStreamModal.call({ session })) reloadSessions();
  };
  // Map each streaming user to their uploaded avatar (sessions carry only a name).
  const avatarByUser = useMemo(() => {
    const m = new Map<string, string | null>();
    for (const u of usersData?.users ?? []) m.set(u.id, u.avatarUrl ?? null);
    return m;
  }, [usersData]);

  return (
    <>
      <PageHeader
        title={serverInfo?.name ?? 'KROMA'}
        suffix={t('admin.dashboardSuffix')}
        realtime
      />

      <Section title={t('admin.nowPlaying')}>
        {sessions.length === 0 ? (
          <EmptyState
            icon={<IconPlayerPlay size={32} stroke={1.5} />}
            title={t('admin.noPlayback')}
          />
        ) : (
          <div className="flex flex-col gap-3.5">
            {sessions.map((s) => (
              <NowPlayingCard
                key={s.id}
                s={s}
                avatarUrl={s.userId ? avatarByUser.get(s.userId) : null}
                onStop={() => void askStop(s)}
              />
            ))}
          </div>
        )}
      </Section>

      <BandwidthSection metrics={metrics} />
      <CpuSection metrics={metrics} />
      <RamSection metrics={metrics} />

      <Section
        title={t('admin.topUsers')}
        right={
          <RangeSelect
            ariaLabel={t('admin.topUsers')}
            value={topDays}
            onChange={setTopDays}
            options={[7, 30, 90]}
          />
        }
      >
        {top && top.users.length > 0 ? (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            {top.users.slice(0, 3).map((u) => (
              <TopUserCard key={u.username} u={u} />
            ))}
          </div>
        ) : (
          <EmptyState icon={<IconUsers size={32} stroke={1.5} />} title={t('admin.noHistory')} />
        )}
      </Section>

      <Section
        title={t('admin.playHistory')}
        right={
          <RangeSelect
            ariaLabel={t('admin.playHistory')}
            value={historyDays}
            onChange={setHistoryDays}
            options={[30, 90, 180]}
          />
        }
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
    <Section
      title={t('admin.bandwidth')}
      right={<FilterLabel plain>{t('admin.realtime')}</FilterLabel>}
    >
      <MetricsChart
        max={max}
        sampleSec={sampleSec(metrics)}
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
  const kroma = metrics?.series.cpuKroma ?? [];
  const sys = metrics?.series.cpuSystem ?? [];
  return (
    <Section title={t('admin.cpu')} right={<FilterLabel plain>{t('admin.realtime')}</FilterLabel>}>
      <MetricsChart
        max={100}
        sampleSec={sampleSec(metrics)}
        formatValue={pct}
        series={[
          { label: t('admin.legendSystem'), data: sys, color: C.cpuRed },
          { label: t('admin.legendKromaServer'), data: kroma, color: C.green },
        ]}
        legend={[
          { label: t('admin.legendKromaServer'), color: C.green },
          { label: t('admin.legendSystem'), color: C.cpuRed },
        ]}
        footer={t('admin.cpuAverages', {
          kroma: decimal(avg(kroma), 1),
          sys: decimal(avg(sys), 1),
        })}
      />
    </Section>
  );
}

function RamSection({ metrics }: Readonly<{ metrics: MetricsSnapshot | null }>) {
  const t = useT();
  const kroma = metrics?.series.ramKroma ?? [];
  const sys = metrics?.series.ramSystem ?? [];
  return (
    <Section title={t('admin.ram')} right={<FilterLabel plain>{t('admin.realtime')}</FilterLabel>}>
      <MetricsChart
        max={100}
        sampleSec={sampleSec(metrics)}
        formatValue={pct}
        series={[
          { label: t('admin.legendSystem'), data: sys, color: C.purple },
          { label: t('admin.legendKromaServer'), data: kroma, color: C.green },
        ]}
        legend={[
          { label: t('admin.legendKromaServer'), color: C.green },
          { label: t('admin.legendSystem'), color: C.purple },
        ]}
        footer={t('admin.ramAverages', {
          kroma: decimal(avg(kroma), 2),
          sys: decimal(avg(sys), 2),
        })}
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
              style={{ color: r.on ? C.accent : 'var(--kroma-text-muted)' }}
            >
              {r.label}
            </span>
            <span
              className="text-[13.5px] font-semibold tabular-nums"
              style={{ color: r.on ? C.accent : 'var(--kroma-text-muted)' }}
            >
              {r.val}
            </span>
          </div>
        ))}
      </div>
    </Card>
  );
}
