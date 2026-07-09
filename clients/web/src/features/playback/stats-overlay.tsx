import type { AudioTrack } from '@luma/core';
import { useT } from '@luma/ui';
import { type RefObject, useEffect, useState } from 'react';
import { IconClose } from '#web/features/playback/icons';
import type { MovieView } from '#web/shared/lib/api';

const READY = ['HAVE_NOTHING', 'HAVE_METADATA', 'HAVE_CURRENT', 'HAVE_FUTURE', 'HAVE_ENOUGH'];
const NETWORK = ['EMPTY', 'IDLE', 'LOADING', 'NO_SOURCE'];

interface ConnLike {
  downlink?: number;
  effectiveType?: string;
}

/** Format seconds as `H:MM:SS` (or `M:SS`). */
function clock(s: number): string {
  if (!Number.isFinite(s) || s < 0) s = 0;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = Math.floor(s % 60);
  const mm = h ? String(m).padStart(2, '0') : String(m);
  return `${h ? `${h}:` : ''}${mm}:${String(sec).padStart(2, '0')}`;
}

/** YouTube-style "stats for nerds": codec, decoded vs display resolution, buffer
 * health, dropped frames, average bitrate, connection refreshed live. Plus the
 * HLS transport state (mode, remux anchor, relative element clock, seekable +
 * buffered ranges) to diagnose seek / re-anchor behaviour. */
export function StatsOverlay({
  videoRef,
  item,
  cur,
  dur,
  bufEnd,
  anchor,
  baseSec,
  useHls,
  aac,
  audioTracks,
  audioIndex,
  hlsRef,
  onClose,
}: Readonly<{
  videoRef: RefObject<HTMLVideoElement | null>;
  item: MovieView;
  cur: number;
  dur: number;
  bufEnd: number;
  /** HLS remux anchor (s, absolute) the stream is started from. */
  anchor: number;
  /** Absolute-position offset (= anchor for HLS, 0 for direct). */
  baseSec: number;
  /** Playing via the HLS remux (vs direct-play). */
  useHls: boolean;
  /** HLS audio is re-encoded to AAC (vs stream-copied). */
  aac: boolean;
  /** All audio tracks + the selected one (audio-relative index). */
  audioTracks: AudioTrack[];
  audioIndex: number;
  /** The live hls.js instance, to read the ACTUALLY-playing audio rendition. */
  hlsRef: { current: import('hls.js').default | null };
  onClose: () => void;
}>) {
  const t = useT();
  const [, force] = useState(0);
  const [bytes, setBytes] = useState(0);

  // Re-render 1×/s to keep the live counters fresh (2×/s doubled the repaint
  // cost during playback for values that barely change within a second).
  useEffect(() => {
    const id = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  // One-shot: learn the total file size (avg bitrate) from a tiny range request.
  useEffect(() => {
    let cancelled = false;
    fetch(item.stream, { headers: { Range: 'bytes=0-1' } })
      .then((r) => {
        const cr = r.headers.get('Content-Range');
        const total = cr ? Number(cr.split('/')[1]) : Number(r.headers.get('Content-Length') ?? 0);
        if (!cancelled && Number.isFinite(total) && total > 0) setBytes(total);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [item.stream]);

  const v = videoRef.current;
  const vw = v?.videoWidth || item.video?.width || 0;
  const vh = v?.videoHeight || item.video?.height || 0;
  const dpr = typeof window !== 'undefined' ? window.devicePixelRatio : 1;
  const dw = v ? Math.round(v.clientWidth * dpr) : 0;
  const dh = v ? Math.round(v.clientHeight * dpr) : 0;
  const q = v?.getVideoPlaybackQuality?.();
  const dropped = q?.droppedVideoFrames ?? 0;
  const totalFrames = q?.totalVideoFrames ?? 0;
  const dropPct = totalFrames ? ((dropped / totalFrames) * 100).toFixed(2) : '0';
  const bufferAhead = Math.max(0, bufEnd - cur);
  const avgMbps = bytes && dur ? (bytes * 8) / dur / 1e6 : 0;
  const conn =
    (typeof navigator !== 'undefined'
      ? (navigator as Navigator & { connection?: ConnLike }).connection
      : undefined) ?? {};
  const vcodec = item.video?.codec?.toUpperCase() ?? '-';
  // The SELECTED audio track (matches the menu), not the item default.
  const selAudio = audioTracks.find((a) => a.index === audioIndex) ?? audioTracks[0];
  const acodec = selAudio?.codec?.toUpperCase() ?? item.audio?.codec?.toUpperCase() ?? '-';
  // What hls.js is ACTUALLY playing (its active rendition's language), to catch a
  // selection ≠ playback mismatch.
  const hls = hlsRef.current;
  const hlsTracks = hls?.audioTracks as Array<{ lang?: string; name?: string }> | undefined;
  const activeAudio =
    hls && hlsTracks && hls.audioTrack >= 0
      ? (hlsTracks[hls.audioTrack]?.lang ?? hlsTracks[hls.audioTrack]?.name ?? `#${hls.audioTrack}`)
      : null;

  // HLS transport: the element clock is RELATIVE to the remux anchor, so show
  // both the absolute position and the raw relative time + the seekable/buffered
  // RELATIVE ranges (a gap between ranges = a stall/hole).
  const rel = v?.currentTime ?? 0;
  const seekRel = v?.seekable.length ? v.seekable.end(v.seekable.length - 1) : 0;
  const nRanges = v?.buffered.length ?? 0;
  let rangeStr = '-';
  if (v && nRanges > 0) {
    const parts: string[] = [];
    for (let i = 0; i < nRanges; i += 1) {
      parts.push(`${v.buffered.start(i).toFixed(0)}–${v.buffered.end(i).toFixed(0)}`);
    }
    rangeStr = parts.join(' ');
  }
  const playbackMode = useHls ? `HLS · ${aac ? 'AAC' : 'copy'}` : 'Direct';

  const rows: [string, string][] = [
    [t('stats.title2'), item.title],
    [t('stats.id'), item.id],
    [t('stats.container'), item.container.toUpperCase()],
    [t('stats.playback'), playbackMode],
    [t('stats.position'), useHls ? `${clock(cur)} · rel ${rel.toFixed(0)}s` : clock(cur)],
    ...(useHls
      ? ([
          [t('stats.anchor'), `${clock(anchor)} (${baseSec.toFixed(0)}s)`],
          [t('stats.seekable'), `0–${seekRel.toFixed(0)}s`],
          [t('stats.ranges'), `${nRanges} · ${rangeStr}`],
        ] as [string, string][])
      : []),
    [
      t('stats.video'),
      `${vcodec}${item.video?.bitDepth ? ` ${item.video.bitDepth}-bit` : ''}${item.video?.hdr ? ' HDR' : ''}`,
    ],
    [
      t('stats.audio'),
      `${acodec}${selAudio?.channels ? ` ${selAudio.channels}.0` : ''}${selAudio?.language ? ` (${selAudio.language})` : ''}`,
    ],
    ...(useHls && activeAudio
      ? ([[t('stats.audioActive'), `#${hls?.audioTrack} · ${activeAudio}`]] as [string, string][])
      : []),
    [t('stats.resolution'), vw && vh ? `${vw}×${vh}` : '-'],
    [t('stats.display'), dw && dh ? `${dw}×${dh} @${dpr}x` : '-'],
    [t('stats.avgBitrate'), avgMbps ? `${avgMbps.toFixed(2)} Mb/s` : '…'],
    [t('stats.size'), bytes ? `${(bytes / 1e9).toFixed(2)} Go` : '…'],
    [t('stats.buffer'), t('stats.bufferAhead', { seconds: bufferAhead.toFixed(1) })],
    [t('stats.droppedFrames'), `${dropped} / ${totalFrames} (${dropPct}%)`],
    [
      t('stats.volume'),
      `${Math.round((v?.volume ?? 1) * 100)}%${v?.muted ? t('stats.volumeMuted') : ''}`,
    ],
    [t('stats.speed'), `${v?.playbackRate ?? 1}×`],
    [t('stats.state'), `${READY[v?.readyState ?? 0]} · NET_${NETWORK[v?.networkState ?? 0]}`],
    [
      t('stats.connection'),
      conn.downlink ? `${conn.downlink} Mb/s · ${conn.effectiveType ?? ''}` : '-',
    ],
  ];

  return (
    <div className="pointer-events-auto absolute left-8 top-24 z-65 w-82.5 rounded-xl border border-white/10 bg-black/72 p-3.5 font-mono text-[12px] text-white/85 shadow-pop backdrop-blur-md">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[11px] font-bold uppercase tracking-[.14em] text-accent">
          {t('stats.title')}
        </span>
        <button
          onClick={onClose}
          className="text-white/60 hover:text-white"
          aria-label={t('common.close')}
        >
          <IconClose size={15} />
        </button>
      </div>
      <div className="flex flex-col gap-0.75">
        {rows.map(([k, val]) => (
          <div key={k} className="flex justify-between gap-4">
            <span className="text-white/45">{k}</span>
            <span className="truncate text-right tabular-nums">{val}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
