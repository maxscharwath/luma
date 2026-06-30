import { audioSupport, type MediaItem, metaLine } from '@luma/core';
import { useLocale, useT } from '@luma/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useClient, useNav, useParams } from '#tv/app/router';
import { endsAtClock } from '#tv/features/catalog/detail/parts';
import { AvPanel } from '#tv/features/playback/player/AvPanel';
import { ControlBar } from '#tv/features/playback/player/ControlBar';
import { BackChevron } from '#tv/features/playback/player/icons';
import { SkipIntroButton, UpNextCard } from '#tv/features/playback/player/PlayerOverlays';
import { useDirectPlayback } from '#tv/features/playback/player/useDirectPlayback';
import { usePlayerControls } from '#tv/features/playback/player/usePlayerControls';
import { useSubtitleSelection } from '#tv/features/playback/player/useSubtitleSelection';
import { TvSubtitles } from '#tv/features/playback/TvSubtitles';

/** No credits marker → assume the last `CREDITS_TAIL`s are the credits. */
const CREDITS_TAIL = 30;
/** Netflix-style fixed auto-advance countdown (s) once the credits zone opens. */
const AUTO_NEXT = 12;

/**
 * Fullscreen 10-foot direct-play surface. Composes playback (useDirectPlayback),
 * subtitle tracks (useSubtitleSelection) and the remote-driven control overlay
 * (usePlayerControls), plus the skip-intro / up-next chapter affordances. The
 * chrome lives in ControlBar + PlayerOverlays; this is the orchestration.
 */
export function TvPlayer() {
  const nav = useNav();
  const { item } = useParams('player');
  const client = useClient();
  const t = useT();
  const locale = useLocale();

  const playback = useDirectPlayback(client, item);
  const subs = useSubtitleSelection(client, item);
  const { cur, dur, bufEnd, playing, waiting, error, terminated, verdict, seekPreview } = playback;

  // "Up next" (series autoplay): the next episode + a one-shot advance guard.
  // Reset per item so a replaced player screen starts clean.
  const [next, setNext] = useState<MediaItem | null>(null);
  const [upNextCancelled, setUpNextCancelled] = useState(false);
  const advancedRef = useRef(false);
  useEffect(() => {
    setNext(null);
    setUpNextCancelled(false);
    advancedRef.current = false;
    client
      .nextEpisode(item.id)
      .then(setNext)
      .catch(() => undefined);
  }, [client, item.id]);
  const goNext = useCallback(() => {
    if (advancedRef.current || !next) return;
    advancedRef.current = true;
    nav.replace('player', { item: next });
  }, [next, nav]);
  const cancelUpNext = useCallback(() => setUpNextCancelled(true), []);

  // Intro / credits chapter markers (episodes only).
  const intro = useMemo(() => (item.markers ?? []).find((m) => m.kind === 'intro'), [item.markers]);
  const credits = useMemo(
    () => (item.markers ?? []).find((m) => m.kind === 'credits'),
    [item.markers],
  );
  // Skip-Intro window: only inside the marked intro segment.
  const canSkipIntro = Boolean(intro && cur * 1000 >= intro.startMs && cur * 1000 < intro.endMs);
  const skipIntro = useCallback(() => {
    const v = playback.videoRef.current;
    if (intro && v) v.currentTime = intro.endMs / 1000;
  }, [intro, playback.videoRef]);

  // "Up next" trigger: the credits marker, else the last CREDITS_TAIL seconds.
  // `creditsAt > 0` guards short clips (dur <= CREDITS_TAIL) whose tail offset
  // would otherwise collapse to 0 and show the card (+countdown) from the start.
  const creditsAt = credits ? credits.startMs / 1000 : dur - CREDITS_TAIL;
  const showUpNext =
    Boolean(next) && !upNextCancelled && terminated == null && creditsAt > 0 && cur >= creditsAt;

  const audioOptions = useMemo(
    () => playback.audioTracks.map((a) => a.index),
    [playback.audioTracks],
  );
  const {
    controls,
    zone,
    avOpen,
    avFocus,
    barFocusName,
    skipFocused,
    upNextPlayFocus,
    upNextCancelFocus,
  } = usePlayerControls({
    playing: playback.playing,
    togglePlay: playback.togglePlay,
    seek: playback.seek,
    nudge: playback.nudge,
    onExit: nav.back,
    audioOptions,
    activeAudio: playback.audioIndex,
    pickAudio: playback.setAudio,
    subOptions: subs.options,
    pickSub: subs.pick,
    hasNext: Boolean(next),
    onNext: goNext,
    canSkipIntro,
    onSkipIntro: skipIntro,
    upNext: showUpNext,
    onUpNextPlay: goNext,
    onUpNextCancel: cancelUpNext,
  });

  const audio = useMemo(() => audioSupport(item), [item]);
  const subtitle =
    item.kind === 'episode' && item.showTitle
      ? `${item.showTitle} · S${item.season}E${item.episode}`
      : metaLine(item);

  // While progressively seeking, the bar + time preview the pending position.
  const shown = seekPreview ?? cur;
  const pct = dur ? (shown / dur) * 100 : 0;
  const bufPct = dur ? (bufEnd / dur) * 100 : 0;

  // Fixed Netflix-style countdown once the credits zone opens. It only advances
  // at 0 (never on a raw position check) and is frozen during a scrub, so
  // seeking near the end can never teleport to the next episode.
  const [countdown, setCountdown] = useState(AUTO_NEXT);
  useEffect(() => {
    if (!showUpNext) {
      setCountdown(AUTO_NEXT);
      return;
    }
    if (seekPreview != null) return; // frozen mid-scrub
    if (countdown <= 0) {
      goNext();
      return;
    }
    const id = setTimeout(() => setCountdown((c) => c - 1), 1000);
    return () => clearTimeout(id);
  }, [showUpNext, seekPreview, countdown, goNext]);

  // Auto-advance fallback on the real `ended` event (seeking near the end must
  // NOT teleport to the next episode the countdown above is the primary path).
  useEffect(() => {
    const v = playback.videoRef.current;
    if (!v || !next || upNextCancelled) return;
    const onEnded = () => goNext();
    v.addEventListener('ended', onEnded);
    return () => v.removeEventListener('ended', onEnded);
  }, [playback.videoRef, next, upNextCancelled, goNext]);

  const endsAt = dur ? endsAtClock(Math.max(0, dur - cur) * 1000, locale) : '';
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

      {/* "Up next" card at the credits, with a countdown + Play now / Cancel. */}
      <UpNextCard
        show={showUpNext}
        next={next}
        client={client}
        countdown={countdown}
        playFocused={upNextPlayFocus}
        cancelFocused={upNextCancelFocus}
        onPlay={goNext}
        onCancel={cancelUpNext}
      />

      {/* Skip-Intro: auto-focused for the whole intro window, OK skips. */}
      <SkipIntroButton visible={canSkipIntro} focused={skipFocused} onSkip={skipIntro} />

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

      <ControlBar
        fade={fade}
        zone={zone}
        controls={controls}
        seekPreview={seekPreview}
        shown={shown}
        dur={dur}
        pct={pct}
        bufPct={bufPct}
        endsAt={endsAt}
        playing={playing}
        hasNext={Boolean(next)}
        showCountdown={showUpNext}
        ringProgress={countdown / AUTO_NEXT}
        markers={item.markers}
        barFocusName={barFocusName}
      />

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
