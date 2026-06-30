import type { Marker } from '@luma/core';
import { useT } from '@luma/ui';
import { fmtTime } from '#tv/features/playback/player/fmt';
import {
  ForwardGlyph,
  PauseGlyph,
  PlayGlyph,
  RewindGlyph,
  TracksGlyph,
} from '#tv/features/playback/player/icons';
import {
  CTRL,
  CTRL_OFF,
  CTRL_ON,
  FOCUS_RING,
  PILL,
} from '#tv/features/playback/player/playerStyles';
import type { Zone } from '#tv/features/playback/player/usePlayerControls';

/** Skip-to-next-episode glyph (⏭). */
function NextGlyph() {
  return (
    <svg width="30" height="30" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M6 5l9 7-9 7V5zm11 0h2v14h-2V5z" />
    </svg>
  );
}

/** Draining accent ring around the Play control during the auto-next countdown. */
function CountdownRing({ progress }: { progress: number }) {
  const c = 2 * Math.PI * 46;
  return (
    <svg
      className="pointer-events-none absolute -inset-1.5"
      viewBox="0 0 100 100"
      aria-hidden="true"
    >
      <circle cx="50" cy="50" r="46" fill="none" stroke="rgba(0,0,0,0.3)" strokeWidth="5" />
      <circle
        cx="50"
        cy="50"
        r="46"
        fill="none"
        stroke="var(--luma-accent-bright)"
        strokeWidth="5"
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={c * (1 - Math.max(0, Math.min(1, progress)))}
        transform="rotate(-90 50 50)"
      />
    </svg>
  );
}

interface ControlBarProps {
  fade: string;
  zone: Zone;
  controls: boolean;
  seekPreview: number | null;
  /** Position shown on the bar (the pending seek target while scrubbing). */
  shown: number;
  dur: number;
  pct: number;
  bufPct: number;
  endsAt: string;
  playing: boolean;
  hasNext: boolean;
  /** Up-next countdown is active → draw the ring around Play. */
  showCountdown: boolean;
  /** Countdown ring fill, 1 → 0 over the auto-advance window. */
  ringProgress: number;
  /** Intro / credits segments to mark on the scrub track. */
  markers?: readonly Marker[];
  barFocusName: (name: string) => boolean;
}

/** Bottom seek bar + the focusable control row + the remote hint. */
export function ControlBar({
  fade,
  zone,
  controls,
  seekPreview,
  shown,
  dur,
  pct,
  bufPct,
  endsAt,
  playing,
  hasNext,
  showCountdown,
  ringProgress,
  markers,
  barFocusName,
}: ControlBarProps) {
  const t = useT();
  return (
    <div
      className={`absolute inset-x-0 bottom-0 bg-[linear-gradient(0deg,rgba(0,0,0,0.82),transparent)] px-8.5 pb-7 transition-opacity duration-350 ${fade}`}
    >
      <div className="mb-5 flex items-center gap-4">
        <span
          className={`w-16 font-sans text-[15px] font-semibold tabular-nums ${
            seekPreview != null ? 'text-accent' : 'text-[rgba(244,243,240,0.85)]'
          }`}
        >
          {fmtTime(shown)}
        </span>
        <div
          className={`relative flex-1 rounded-full bg-[rgba(255,255,255,0.18)] transition-[height,box-shadow] duration-200 ${
            zone === 'progress' && controls
              ? 'h-2.5 shadow-[0_0_0_4px_rgba(242,180,66,0.35)]'
              : 'h-1.5'
          }`}
        >
          <div
            className="absolute inset-y-0 left-0 rounded-full bg-[rgba(255,255,255,0.14)]"
            style={{ width: `${bufPct}%` }}
          />
          {dur > 0
            ? markers?.map((m) => {
                const durMs = dur * 1000;
                const left = Math.max(0, Math.min(100, (m.startMs / durMs) * 100));
                const width = Math.max(0.6, ((m.endMs - m.startMs) / durMs) * 100);
                return (
                  <div
                    key={m.kind}
                    className="absolute inset-y-0 rounded-full"
                    style={{
                      left: `${left}%`,
                      width: `${width}%`,
                      background:
                        m.kind === 'intro' ? 'rgba(120,180,255,0.65)' : 'rgba(214,140,255,0.65)',
                    }}
                  />
                );
              })
            : null}
          <div
            className="absolute inset-y-0 left-0 rounded-full bg-[linear-gradient(90deg,var(--luma-accent),var(--luma-accent-bright))] shadow-[0_0_12px_rgba(242,180,66,0.55)]"
            style={{ width: `${pct}%` }}
          />
          <div
            className={`absolute top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white shadow-[0_0_0_4px_rgba(242,180,66,0.4),0_2px_6px_rgba(0,0,0,0.5)] transition-[width,height] duration-200 ${
              zone === 'progress' && controls ? 'h-4.75 w-4.75' : 'h-3.75 w-3.75'
            }`}
            style={{ left: `${pct}%` }}
          />
        </div>
        <span className="w-16 text-right font-sans text-[15px] font-semibold text-[rgba(244,243,240,0.55)] tabular-nums">
          {fmtTime(dur)}
        </span>
        {endsAt ? (
          <span className="whitespace-nowrap font-sans text-[13px] font-semibold text-[rgba(244,243,240,0.42)] tabular-nums">
            {t('content.endsAtShort', { time: endsAt })}
          </span>
        ) : null}
      </div>

      <div className="flex items-center justify-center gap-5.5 pt-1">
        <div
          className={`${CTRL} h-17.5 w-17.5 ${barFocusName('rewind') ? `${FOCUS_RING} ${CTRL_ON}` : CTRL_OFF}`}
        >
          <RewindGlyph />
        </div>
        <div
          className={`${CTRL} relative h-21 w-21 text-accent-ink ${barFocusName('play') ? `${FOCUS_RING} bg-accent-hover` : 'bg-accent'}`}
        >
          {playing ? <PauseGlyph /> : <PlayGlyph />}
          {showCountdown ? <CountdownRing progress={ringProgress} /> : null}
        </div>
        <div
          className={`${CTRL} h-17.5 w-17.5 ${barFocusName('forward') ? `${FOCUS_RING} ${CTRL_ON}` : CTRL_OFF}`}
        >
          <ForwardGlyph />
        </div>
        {hasNext ? (
          <div
            className={`${CTRL} h-17.5 w-17.5 ${barFocusName('next') ? `${FOCUS_RING} ${CTRL_ON}` : CTRL_OFF}`}
            aria-label={t('player.nextEpisode')}
          >
            <NextGlyph />
          </div>
        ) : null}
        <div className={`${PILL} ${barFocusName('av') ? `${FOCUS_RING} ${CTRL_ON}` : CTRL_OFF}`}>
          <TracksGlyph />
          {t('player.audioSubShort')}
        </div>
      </div>

      <div className="mt-4 text-center font-sans text-[14px] font-semibold text-dim">
        {t('player.hint')}
      </div>
    </div>
  );
}
