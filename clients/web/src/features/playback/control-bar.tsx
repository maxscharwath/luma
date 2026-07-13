import { formatTimecode as fmtTime, type Marker } from '@luma/core';
import { useT } from '@luma/ui';
import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import * as Slider from '@radix-ui/react-slider';
import {
  IconBack10,
  IconCheck,
  IconFullscreen,
  IconFullscreenExit,
  IconFwd10,
  IconMute,
  IconPause,
  IconPip,
  IconPlay,
  IconStats,
  IconTracks,
  IconVolume,
} from '#web/features/playback/icons';
import type { Storyboard } from '#web/features/playback/use-storyboard';
import type { VideoPlayback } from '#web/features/playback/use-video-playback';

const RATES = [0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];

/** Width (px) of the scrub-bar hover-preview thumbnail; height follows 16:9. */
const PREVIEW_W = 168;

/** Auto-hiding bottom control surface: scrub bar (with hover preview + buffered
 * track) and the transport / volume / speed / stats / tracks / PiP / fullscreen row. */
/** Skip-to-next-episode glyph (⏭). */
function IconNext() {
  return (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M6 5l9 7-9 7V5zm11 0h2v14h-2V5z" />
    </svg>
  );
}

export function ControlBar({
  pb,
  storyboard,
  statsOpen,
  markers,
  onToggleStats,
  onOpenAv,
  onPlayNext,
}: Readonly<{
  pb: VideoPlayback;
  /** Scrub-bar hover-thumbnail sheet (YouTube-style preview). */
  storyboard: Storyboard;
  statsOpen: boolean;
  /** Intro / credits segments to mark on the scrub track. */
  markers?: readonly Marker[];
  onToggleStats: () => void;
  onOpenAv: () => void;
  /** Skip to the next episode (series only); omitted = no button. */
  onPlayNext?: () => void;
}>) {
  const t = useT();
  const { cur, dur, bufEnd, playing, muted, volume, rate, fs, hover, scrubPreview } = pb;
  // While dragging, the bar/thumb follow the preview; the seek commits on release.
  const shown = scrubPreview ?? cur;
  const pct = dur ? (shown / dur) * 100 : 0;
  const bufPct = dur ? (bufEnd / dur) * 100 : 0;
  // Thumbnail under the cursor; null until the sheet is ready (→ time label only).
  const previewTile = hover ? storyboard.tile(hover.t, PREVIEW_W) : null;
  // Keep the floating preview inside the bar instead of overflowing at the ends.
  const previewLeft = hover
    ? Math.max(PREVIEW_W / 2, Math.min(hover.w - PREVIEW_W / 2, hover.x))
    : 0;

  return (
    <>
      {/* scrub row */}
      <div className="mb-3.5 flex items-center gap-3.5">
        <span className="w-13.5 text-[13px] font-semibold tabular-nums text-white/85">
          {fmtTime(shown)}
        </span>
        <div
          ref={pb.barRef}
          className="relative flex h-5 flex-1 cursor-pointer items-center pointer-coarse:h-7"
          onPointerDown={(e) => {
            (e.target as Element).setPointerCapture?.(e.pointerId);
            pb.setScrubbing(true);
            pb.scrubToClientX(e.clientX); // preview; commit on pointer up
          }}
          onPointerMove={pb.onBarMove}
          onPointerUp={() => {
            pb.commitScrub();
            pb.setScrubbing(false);
          }}
          onPointerCancel={() => {
            pb.commitScrub();
            pb.setScrubbing(false);
          }}
          onPointerLeave={() => pb.setHover(null)}
        >
          {hover ? (
            <div
              className="pointer-events-none absolute bottom-7 z-10 flex -translate-x-1/2 flex-col items-center gap-1.5"
              style={{ left: previewLeft }}
            >
              {previewTile ? (
                <div
                  className="overflow-hidden rounded-lg border border-white/20 bg-black shadow-[0_8px_24px_rgba(0,0,0,.6)] ring-1 ring-black/40"
                  style={previewTile as React.CSSProperties}
                />
              ) : null}
              <span className="rounded-md bg-black/75 px-2.5 py-1 text-[12px] font-semibold tabular-nums text-white">
                {fmtTime(hover.t)}
              </span>
            </div>
          ) : null}
          <div className="relative h-1.25 w-full rounded-full bg-white/20">
            <div
              className="absolute inset-y-0 left-0 rounded-full bg-white/15"
              style={{ width: `${bufPct}%` }}
            />
            {/* Intro / credits segments from the markers DTO. */}
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
                      title={m.kind === 'intro' ? t('player.skipIntro') : t('content.upNext')}
                    />
                  );
                })
              : null}
            <div
              className="absolute inset-y-0 left-0 rounded-full bg-linear-to-r from-accent to-[#FFD262] shadow-[0_0_12px_rgba(242,180,66,.55)]"
              style={{ width: `${pct}%` }}
            />
            <div
              className="absolute top-1/2 h-3.75 w-3.75 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white shadow-[0_0_0_4px_rgba(242,180,66,.4),0_2px_6px_rgba(0,0,0,.5)]"
              style={{ left: `${pct}%` }}
            />
          </div>
        </div>
        <span className="w-13.5 text-right text-[13px] font-semibold tabular-nums text-white/55">
          {fmtTime(dur)}
        </span>
      </div>

      {/* button row (speed / stats / PiP are hidden below md so it fits phones) */}
      <div className="flex items-center gap-1 sm:gap-2">
        <CtrlButton onClick={pb.togglePlay} label={playing ? t('player.pause') : t('player.play')}>
          {playing ? <IconPause /> : <IconPlay />}
        </CtrlButton>
        <CtrlButton onClick={() => pb.skip(-10)} label={t('player.back10')}>
          <IconBack10 />
        </CtrlButton>
        <CtrlButton onClick={() => pb.skip(10)} label={t('player.fwd10')}>
          <IconFwd10 />
        </CtrlButton>
        {onPlayNext ? (
          <CtrlButton onClick={onPlayNext} label={t('player.nextEpisode')}>
            <IconNext />
          </CtrlButton>
        ) : null}

        {/* volume */}
        <div className="group flex items-center">
          <CtrlButton onClick={pb.toggleMute} label={t('player.mute')}>
            {muted || volume === 0 ? <IconMute /> : <IconVolume />}
          </CtrlButton>
          {/* Slider slides out on hover/focus; on touch the button is a mute toggle only. */}
          <div className="w-0 overflow-hidden opacity-0 transition-all duration-200 group-focus-within:w-24 group-focus-within:opacity-100 group-hover:w-24 group-hover:opacity-100 pointer-coarse:hidden">
            <Slider.Root
              className="relative flex h-9 w-20 touch-none select-none items-center px-2"
              value={[muted ? 0 : volume * 100]}
              max={100}
              step={1}
              onValueChange={([v]) => pb.setVol((v ?? 0) / 100)}
              aria-label={t('player.volume')}
            >
              <Slider.Track className="relative h-1 grow rounded-full bg-white/25">
                <Slider.Range className="absolute h-full rounded-full bg-white" />
              </Slider.Track>
              <Slider.Thumb className="block h-3 w-3 rounded-full bg-white shadow" />
            </Slider.Root>
          </div>
        </div>

        <div className="flex-1" />

        {/* playback speed */}
        <DropdownMenu.Root>
          <DropdownMenu.Trigger asChild>
            <button
              type="button"
              className="rounded-lg px-3 py-2 text-[13px] font-semibold text-white hover:bg-white/10 max-md:hidden"
            >
              {rate}×
            </button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content
              sideOffset={8}
              className="z-70 min-w-30 rounded-xl border border-white/10 bg-surface-2/95 p-1.5 shadow-pop backdrop-blur-md"
            >
              {RATES.map((r) => (
                <DropdownMenu.Item
                  key={r}
                  onSelect={() => pb.applyRate(r)}
                  className="flex cursor-pointer items-center justify-between rounded-md px-3 py-2 text-[13px] text-white outline-none data-[highlighted]:bg-white/10"
                >
                  {r === 1 ? t('player.normalSpeed') : `${r}×`}
                  {r === rate ? (
                    <span className="text-accent">
                      <IconCheck />
                    </span>
                  ) : null}
                </DropdownMenu.Item>
              ))}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>

        <button
          type="button"
          onClick={onToggleStats}
          className={`rounded-lg p-2.5 hover:bg-white/10 max-md:hidden ${statsOpen ? 'text-accent' : 'text-white'}`}
          aria-label={t('player.stats')}
        >
          <IconStats />
        </button>

        <button
          type="button"
          onClick={onOpenAv}
          className="flex items-center gap-2 rounded-lg bg-accent px-4 py-2.5 text-[13px] font-semibold text-accent-ink hover:bg-accent-hover"
        >
          <IconTracks />
          {t('player.audioSubShort')}
        </button>

        <CtrlButton onClick={pb.togglePip} label={t('player.pip')} className="max-md:hidden">
          <IconPip />
        </CtrlButton>
        <CtrlButton onClick={pb.toggleFullscreen} label={t('player.fullscreen')}>
          {fs ? <IconFullscreenExit /> : <IconFullscreen />}
        </CtrlButton>
      </div>
    </>
  );
}

function CtrlButton({
  onClick,
  label,
  className,
  children,
}: Readonly<{
  onClick: () => void;
  label: string;
  className?: string;
  children: React.ReactNode;
}>) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className={`flex h-11 w-11 items-center justify-center rounded-lg text-white hover:bg-white/10 ${className ?? ''}`}
    >
      {children}
    </button>
  );
}
