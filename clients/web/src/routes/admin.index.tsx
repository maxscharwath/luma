import type { MetricsSnapshot, PlaybackSession, TopUser } from '@luma/core';
import { useT } from '@luma/ui';
import {
  IconPlayerPauseFilled,
  IconPlayerPlayFilled,
  IconPlayerStopFilled,
} from '@tabler/icons-react';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { HistoryBars, MetricsChart } from '#web/components/admin/charts';
import { PageHeader, useAdmin, usePoll } from '#web/components/admin/shell';
import {
  Avatar,
  C,
  Card,
  FilterLabel,
  Modal,
  ProgressBar,
  Section,
} from '#web/components/admin/ui';
import {
  decimal,
  formatDuration,
  formatMbps,
  posterGradient,
  timecode,
} from '#web/lib/adminFormat';
import { useAuth } from '#web/lib/auth';

export const Route = createFileRoute('/admin/')({
  component: DashboardPage,
});

function DashboardPage() {
  const t = useT();
  const { client } = useAuth();
  const { serverInfo, tick } = useAdmin();

  const { data: sessionsData, reload: reloadSessions } = usePoll(
    () => client.adminSessions(),
    3000,
    [client, tick],
  );
  const { data: metrics } = usePoll(() => client.adminMetrics(), 2000, [client]);
  const { data: top } = usePoll(() => client.topUsers(7), 30000, [client, tick]);
  const { data: history } = usePoll(() => client.playHistory(28), 60000, [client, tick]);

  const [stopTarget, setStopTarget] = useState<PlaybackSession | null>(null);
  const sessions = sessionsData?.sessions ?? [];

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
              <NowPlayingCard key={s.id} s={s} onStop={() => setStopTarget(s)} />
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

// ----- now playing ------------------------------------------------------------

function NowPlayingCard({ s, onStop }: Readonly<{ s: PlaybackSession; onStop: () => void }>) {
  const t = useT();
  const playing = s.state === 'playing';
  const pct = s.durationMs ? (s.positionMs / s.durationMs) * 100 : 0;
  const transcode = s.mode === 'transcode';
  const lan = s.network === 'LAN';
  const stateColor = playing ? C.green : 'rgba(244,243,240,.5)';
  let sub = '';
  if (s.kind === 'episode' && s.season != null)
    sub = t('admin.episodeShort', { season: s.season, episode: s.episode ?? '' });
  else if (s.year != null) sub = String(s.year);

  return (
    <Card className="flex gap-4.5 px-5 py-4.5">
      <div
        className="relative h-22 w-14.5 shrink-0 overflow-hidden rounded-[9px] shadow-[0_8px_20px_rgba(0,0,0,.45)]"
        style={{ background: posterGradient(s.title) }}
      >
        <div className="absolute inset-0 flex items-center justify-center">
          <span className="flex h-6.5 w-6.5 items-center justify-center rounded-full bg-black/55">
            {playing ? (
              <IconPlayerPauseFilled size={11} color="#fff" />
            ) : (
              <IconPlayerPlayFilled size={11} color="#fff" />
            )}
          </span>
        </div>
      </div>

      <div className="flex min-w-0 flex-1 flex-col gap-3">
        <div className="flex items-start justify-between gap-4.5">
          <div className="min-w-0">
            <div className="flex items-center gap-2.5">
              <h3 className="truncate font-display text-[17px] font-bold leading-[1.1]">
                {s.showTitle ? `${s.showTitle}` : s.title}
              </h3>
              <span
                className="inline-flex items-center gap-1.5 text-[10.5px] font-bold"
                style={{ color: stateColor }}
              >
                <span
                  className={`h-1.5 w-1.5 rounded-full ${playing ? 'animate-[luma-breathe_2s_ease-in-out_infinite]' : ''}`}
                  style={{ background: stateColor }}
                />
                {playing ? t('admin.playing') : t('admin.paused')}
              </span>
            </div>
            <div className="mt-1 text-[12.5px] font-medium text-text/50">
              {[sub, s.videoLabel].filter(Boolean).join(' · ')}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2.75">
            <div className="text-right">
              <div className="text-[14px] font-semibold">{s.username}</div>
              <div className="text-[12px] font-medium text-text/50">
                {s.player} · {s.device}
              </div>
            </div>
            <Avatar name={s.username} size={38} radius={10} />
            <button
              type="button"
              onClick={onStop}
              title={t('admin.stopStream')}
              aria-label={t('admin.stopStream')}
              className="flex h-9 w-9 items-center justify-center rounded-md border border-[#E8536A]/25 bg-[#E8536A]/10 text-[#E8536A] transition-colors hover:bg-[#E8536A]/20"
            >
              <IconPlayerStopFilled size={15} />
            </button>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <span className="text-[12px] font-semibold tabular-nums text-text/70">
            {timecode(s.positionMs)}
          </span>
          <div className="flex-1">
            <ProgressBar pct={pct} />
          </div>
          <span className="text-[12px] font-semibold tabular-nums text-text/40">
            {s.durationMs ? timecode(s.durationMs) : '—'}
          </span>
        </div>

        <div className="flex flex-wrap gap-x-6.5 gap-y-2.5 border-t border-border pt-3">
          <Stat label={t('admin.statPlayback')}>
            <span
              className="inline-flex items-center gap-1.5 rounded-[7px] px-2.25 py-0.75 text-[13px] font-semibold"
              style={{
                color: transcode ? C.accent : C.green,
                background: transcode ? 'rgba(242,180,66,.14)' : 'rgba(70,208,141,.14)',
              }}
            >
              {transcode ? t('admin.transcoding') : t('admin.directPlay')}
            </span>
          </Stat>
          <Stat label={t('admin.statVideo')}>
            <span className="text-[13px] font-semibold" style={{ color: C.green }}>
              {s.videoLabel}
            </span>
          </Stat>
          <Stat label={t('admin.statAudioTrack')}>
            <span
              className="text-[13px] font-semibold"
              style={{ color: transcode ? C.accent : C.green }}
            >
              {s.audioLabel}
            </span>
          </Stat>
          <Stat label={t('admin.statSubtitles')}>
            <span className="text-[13px] font-semibold text-text/78">{s.subtitle}</span>
          </Stat>
          <Stat label={t('admin.statBitrate')}>
            <span className="text-[13px] font-semibold tabular-nums text-text/78">
              {formatMbps(s.bitrate)} Mb/s
            </span>
          </Stat>
          <Stat label={t('admin.statNetwork')}>
            <span
              className="inline-flex items-center gap-1.5 rounded-[7px] px-2.25 py-0.75 text-[13px] font-semibold"
              style={{
                color: lan ? C.green : C.blue,
                background: lan ? 'rgba(70,208,141,.12)' : 'rgba(92,141,246,.12)',
              }}
            >
              {s.network} · {s.ip}
            </span>
          </Stat>
        </div>
      </div>
    </Card>
  );
}

function Stat({ label, children }: Readonly<{ label: string; children: React.ReactNode }>) {
  return (
    <div>
      <div className="mb-1 text-[9.5px] font-bold uppercase tracking-[.12em] text-text/38">
        {label}
      </div>
      {children}
    </div>
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

// ----- stop a live stream -----------------------------------------------------

function StopStreamModal({
  session,
  onClose,
  onStopped,
}: Readonly<{ session: PlaybackSession; onClose: () => void; onStopped: () => void }>) {
  const t = useT();
  const { client } = useAuth();
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);

  async function stop() {
    setBusy(true);
    try {
      await client.terminateSession(session.id, message);
      onStopped();
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title={t('admin.stopStreamTitle')} onClose={onClose}>
      <p className="mb-4 text-[13px] text-dim">
        {t('admin.stopStreamDesc', { user: session.username })}
      </p>
      <label className="mb-1.5 block text-[12px] font-bold uppercase tracking-[.12em] text-dim">
        {t('admin.stopMessageLabel')}
      </label>
      <textarea
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        rows={2}
        placeholder={t('admin.stopMessagePlaceholder')}
        className="mb-5 w-full resize-none rounded-lg border border-border-strong bg-surface-2 px-3 py-2.5 text-[14px] outline-none focus:border-accent/60"
      />
      <div className="flex justify-end gap-2.5">
        <button
          type="button"
          onClick={onClose}
          className="rounded-md px-4 py-2.5 text-[14px] font-semibold text-muted"
        >
          {t('common.cancel')}
        </button>
        <button
          type="button"
          onClick={() => void stop()}
          disabled={busy}
          className="rounded-md bg-[#E8536A] px-5 py-2.5 text-[14px] font-bold text-white disabled:opacity-50"
        >
          {busy ? '…' : t('admin.stopStream')}
        </button>
      </div>
    </Modal>
  );
}
