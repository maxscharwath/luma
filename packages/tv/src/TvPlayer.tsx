import { audioSupport, metaLine } from '@luma/core';
import { useT } from '@luma/ui';
import { useMemo } from 'react';
import { AvPanel } from '#tv/player/AvPanel';
import { fmtTime } from '#tv/player/fmt';
import {
  BackChevron,
  ForwardGlyph,
  PauseGlyph,
  PlayGlyph,
  RewindGlyph,
  TracksGlyph,
} from '#tv/player/icons';
import { useDirectPlayback } from '#tv/player/useDirectPlayback';
import { usePlayerControls } from '#tv/player/usePlayerControls';
import { useSubtitleSelection } from '#tv/player/useSubtitleSelection';
import { useClient, useNav, useParams } from '#tv/router';
import { TvSubtitles } from '#tv/TvSubtitles';

const FOCUS_RING = 'scale-[1.07] shadow-[var(--ring-focus),var(--glow-accent)]';
const CTRL =
  'flex items-center justify-center rounded-full text-white transition-[transform,box-shadow,background] duration-180';

/**
 * Fullscreen 10-foot direct-play surface. Composes three concerns: playback
 * (useDirectPlayback), subtitle tracks (useSubtitleSelection) and the remote-driven
 * control overlay (usePlayerControls). The body here is just the render.
 */
export function TvPlayer() {
  const nav = useNav();
  const { item } = useParams('player');
  const client = useClient();
  const t = useT();

  const playback = useDirectPlayback(client, item);
  const subs = useSubtitleSelection(client, item);
  const audioOptions = useMemo(
    () => playback.audioTracks.map((a) => a.index),
    [playback.audioTracks],
  );
  const { controls, zone, avOpen, avFocus, barFocus } = usePlayerControls({
    playing: playback.playing,
    togglePlay: playback.togglePlay,
    seek: playback.seek,
    onExit: nav.back,
    audioOptions,
    activeAudio: playback.audioIndex,
    pickAudio: playback.setAudio,
    subOptions: subs.options,
    pickSub: subs.pick,
  });

  const audio = useMemo(() => audioSupport(item), [item]);
  const subtitle =
    item.kind === 'episode' && item.showTitle
      ? `${item.showTitle} · S${item.season}E${item.episode}`
      : metaLine(item);

  const { cur, dur, bufEnd, playing, waiting, error, terminated, verdict } = playback;
  const pct = dur ? (cur / dur) * 100 : 0;
  const bufPct = dur ? (bufEnd / dur) * 100 : 0;
  const fade = controls ? 'opacity-100' : 'pointer-events-none opacity-0';
  // Warning pill text, by priority: admin stop → stream/codec load error →
  // direct-play verdict → audio support. All resolved to the active locale here.
  let warn: string | null = null;
  if (terminated != null) warn = terminated || t('player.stoppedDefault');
  else if (error) warn = t(error);
  else if (verdict && !verdict.canDirectPlay) warn = t(verdict.messageKey, verdict.messageVars);
  else if (!audio.canPlay && audio.messageKey) warn = t(audio.messageKey, audio.messageVars);

  return (
    <div className={`fixed inset-0 z-60 bg-black ${controls ? '' : 'cursor-none'}`}>
      {/* eslint-disable-next-line jsx-a11y/media-has-caption */}
      <video
        ref={playback.videoRef}
        className="h-full w-full bg-black object-contain"
        autoPlay
        playsInline
      />
      <TvSubtitles
        videoRef={playback.videoRef}
        rendered={subs.rendered}
        activeIndex={subs.active}
        raised={controls}
      />

      {waiting && !error ? (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div className="h-14 w-14 rounded-full border-[3px] border-[rgba(255,255,255,0.2)] border-t-accent animate-[tvp-spin_0.9s_linear_infinite]" />
        </div>
      ) : null}

      {/* top bar */}
      <div
        className={`absolute inset-x-0 top-0 flex items-center gap-4.5 bg-[linear-gradient(180deg,rgba(0,0,0,0.65),transparent)] px-8.5 py-6.5 transition-opacity duration-350 ${fade}`}
      >
        <div className="flex h-10.5 w-10.5 flex-none items-center justify-center rounded-full border border-[rgba(255,255,255,0.14)] bg-[rgba(255,255,255,0.1)] text-white">
          <BackChevron />
        </div>
        <div>
          <div className="font-display text-[22px] font-bold text-white">{item.title}</div>
          <div className="font-sans text-[14px] font-medium text-[rgba(244,243,240,0.6)]">
            {subtitle}
          </div>
        </div>
        {warn ? (
          <div className="ml-auto rounded-full bg-[rgba(242,180,66,0.14)] px-3.5 py-2 font-sans text-[13px] font-semibold text-accent">
            {warn}
          </div>
        ) : null}
      </div>

      {/* bottom controls */}
      <div
        className={`absolute inset-x-0 bottom-0 bg-[linear-gradient(0deg,rgba(0,0,0,0.82),transparent)] px-8.5 pb-7 transition-opacity duration-350 ${fade}`}
      >
        <div className="mb-4.5 flex items-center gap-4">
          <span className="w-16 font-sans text-[15px] font-semibold text-[rgba(244,243,240,0.85)] tabular-nums">
            {fmtTime(cur)}
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
        </div>

        <div className="flex items-center justify-center gap-5.5 pt-1">
          <div
            className={`${CTRL} h-17.5 w-17.5 ${barFocus(0) ? `${FOCUS_RING} bg-[rgba(255,255,255,0.22)]` : 'bg-[rgba(255,255,255,0.12)]'}`}
          >
            <RewindGlyph />
          </div>
          <div
            className={`${CTRL} h-21 w-21 text-accent-ink ${barFocus(1) ? `${FOCUS_RING} bg-accent-hover` : 'bg-accent'}`}
          >
            {playing ? <PauseGlyph /> : <PlayGlyph />}
          </div>
          <div
            className={`${CTRL} h-17.5 w-17.5 ${barFocus(2) ? `${FOCUS_RING} bg-[rgba(255,255,255,0.22)]` : 'bg-[rgba(255,255,255,0.12)]'}`}
          >
            <ForwardGlyph />
          </div>
          <div
            className={`flex h-16 items-center gap-2.75 rounded-full px-7 font-sans text-[18px] font-bold text-white transition-[transform,box-shadow,background] duration-180 ${
              barFocus(3)
                ? `${FOCUS_RING} bg-[rgba(255,255,255,0.22)]`
                : 'bg-[rgba(255,255,255,0.12)]'
            }`}
          >
            <TracksGlyph />
            {t('player.audioSubShort')}
          </div>
        </div>

        <div className="mt-4 text-center font-sans text-[14px] font-semibold text-dim">
          {t('player.hint')}
        </div>
      </div>

      {avOpen ? (
        <AvPanel
          audioTracks={playback.audioTracks}
          audioActive={playback.audioIndex}
          rendered={subs.rendered}
          options={subs.options}
          active={subs.active}
          focus={avFocus}
        />
      ) : null}
    </div>
  );
}
