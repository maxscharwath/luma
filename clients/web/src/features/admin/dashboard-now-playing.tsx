import type { PlaybackSession } from '@kroma/core';
import { Image, useT } from '@kroma/ui';
import { IconPlayerStopFilled } from '@tabler/icons-react';
import { useId, useState } from 'react';
import { createCallable } from 'react-call';
import { Avatar, C, Card, Modal, ProgressBar } from '#web/features/admin/ui';
import { useStoryboard } from '#web/features/playback/use-storyboard';
import { formatMbps, posterGradient, timecode } from '#web/shared/lib/adminFormat';
import { kromaClient } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';

/** Width (px) of the Now Playing thumbnail. Height follows the 16:9 frame so a
 * storyboard tile maps 1:1 its background geometry is computed for this width. */
const THUMB_W = 132;

/**
 * The live session thumbnail: the current storyboard frame (mapped from
 * `positionMs`) when the sheet is ready, else the item poster, else a
 * title-seeded gradient.
 */
function NowPlayingThumb({ s }: Readonly<{ s: PlaybackSession }>) {
  // Never kick/await lazy ffmpeg generation just to paint a 132px thumb: fetch
  // once and only use the sheet if it already exists (else poster / gradient).
  const story = useStoryboard(s.itemId, { generate: false });
  const [posterFailed, setPosterFailed] = useState(false);
  const frame = story.tile(s.positionMs / 1000, THUMB_W);
  const poster = kromaClient().posterUrl(s.itemId);

  return (
    <div
      className="relative aspect-video shrink-0 self-start overflow-hidden rounded-[9px] shadow-[0_8px_20px_rgba(0,0,0,.45)]"
      style={{ width: THUMB_W, background: posterGradient(s.title) }}
    >
      {posterFailed ? null : (
        <Image src={poster} fit="cover" fill onError={() => setPosterFailed(true)} />
      )}
      {frame ? (
        <div
          className="absolute inset-0"
          style={{
            backgroundImage: frame.backgroundImage,
            backgroundPosition: frame.backgroundPosition,
            backgroundSize: frame.backgroundSize,
            backgroundRepeat: frame.backgroundRepeat,
          }}
        />
      ) : null}
    </div>
  );
}

export function NowPlayingCard({
  s,
  avatarUrl,
  onStop,
}: Readonly<{ s: PlaybackSession; avatarUrl?: string | null; onStop: () => void }>) {
  const t = useT();
  const playing = s.state === 'playing';
  const pct = s.durationMs ? (s.positionMs / s.durationMs) * 100 : 0;
  const buffering = s.state === 'buffering';
  // `transcode` = the audio was re-encoded to AAC; `remux` = HLS repackage with
  // both streams copied. Video is NEVER transcoded, so it always reads as direct.
  const transcode = s.mode === 'transcode';
  const remux = s.mode === 'remux';
  const lan = s.network === 'LAN';

  let stateColor = 'rgba(244,243,240,.5)';
  let stateLabel = t('admin.paused');
  if (buffering) {
    stateColor = C.accent;
    stateLabel = t('admin.buffering');
  } else if (playing) {
    stateColor = C.green;
    stateLabel = t('admin.playing');
  }

  // The playback-pipeline badge: direct copy · remux · audio-only transcode.
  let pipe: { color: string; bg: string; label: string } = {
    color: C.green,
    bg: 'rgba(70,208,141,.14)',
    label: t('admin.directPlay'),
  };
  if (transcode)
    pipe = { color: C.accent, bg: 'rgba(242,180,66,.14)', label: t('admin.audioTranscode') };
  else if (remux) pipe = { color: C.blue, bg: 'rgba(92,141,246,.14)', label: t('admin.remux') };
  let sub = '';
  if (s.kind === 'episode' && s.season != null)
    sub = t('admin.episodeShort', { season: s.season, episode: s.episode ?? '' });
  else if (s.year != null) sub = String(s.year);

  return (
    <Card className="flex gap-4.5 px-5 py-4.5">
      <NowPlayingThumb s={s} />

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
                  className={`h-1.5 w-1.5 rounded-full ${playing || buffering ? 'animate-[kroma-breathe_2s_ease-in-out_infinite]' : ''}`}
                  style={{ background: stateColor }}
                />
                {stateLabel}
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
            <Avatar name={s.username} avatarUrl={avatarUrl} size={38} radius={10} />
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
            {s.durationMs ? timecode(s.durationMs) : '-'}
          </span>
        </div>

        <div className="flex flex-wrap gap-x-6.5 gap-y-2.5 border-t border-border pt-3">
          <Stat label={t('admin.statPlayback')}>
            <span
              className="inline-flex items-center gap-1.5 rounded-[7px] px-2.25 py-0.75 text-[13px] font-semibold"
              style={{ color: pipe.color, background: pipe.bg }}
            >
              {pipe.label}
            </span>
          </Stat>
          <Stat label={t('admin.statVideo')}>
            {/* Video is always stream-copied it never gets a transcode badge. */}
            <span className="text-[13px] font-semibold" style={{ color: C.green }}>
              {s.videoLabel}
            </span>
          </Stat>
          <Stat label={t('admin.statAudioTrack')}>
            <span
              className="text-[13px] font-semibold"
              style={{ color: transcode ? C.accent : C.green }}
            >
              {transcode ? `${s.audioLabel} → AAC` : s.audioLabel}
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

/**
 * The "stop this stream" confirmation, as an imperative callable: open it with
 * `await StopStreamModal.call({ session })`, which resolves `true` once the
 * session was terminated (so the caller can refresh) or `false` if dismissed.
 * Its root is mounted once by `AdminModalHosts`; no open-state at the call site.
 */
export const StopStreamModal = createCallable<{ session: PlaybackSession }, boolean>(
  ({ call, session }) => {
    const t = useT();
    const { client } = useAuth();
    const messageId = useId();
    const [message, setMessage] = useState('');
    const [busy, setBusy] = useState(false);

    async function stop() {
      setBusy(true);
      try {
        await client.terminateSession(session.id, message);
        call.end(true);
      } finally {
        setBusy(false);
      }
    }

    return (
      <Modal title={t('admin.stopStreamTitle')} onClose={() => call.end(false)}>
        <p className="mb-4 text-[13px] text-dim">
          {t('admin.stopStreamDesc', { user: session.username })}
        </p>
        <label
          htmlFor={messageId}
          className="mb-1.5 block text-[12px] font-bold uppercase tracking-[.12em] text-dim"
        >
          {t('admin.stopMessageLabel')}
        </label>
        <textarea
          id={messageId}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          rows={2}
          placeholder={t('admin.stopMessagePlaceholder')}
          className="mb-5 w-full resize-none rounded-lg border border-border-strong bg-surface-2 px-3 py-2.5 text-[14px] outline-none focus:border-accent/60"
        />
        <div className="flex justify-end gap-2.5">
          <button
            type="button"
            onClick={() => call.end(false)}
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
  },
);
