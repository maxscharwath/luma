import { audioSupport, type MediaItem, playerSubtitle } from '@luma/core';
import { useLocale, useT } from '@luma/ui';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useClient, useNav, useParams } from '#tv/app/router';
import { endsAtClock } from '#tv/features/catalog/detail/parts';
import { AvPanel } from '#tv/features/playback/player/AvPanel';
import { ControlBar } from '#tv/features/playback/player/ControlBar';
import { BackChevron, StopGlyph } from '#tv/features/playback/player/icons';
import { SkipIntroButton, UpNextCard } from '#tv/features/playback/player/PlayerOverlays';
import { FOCUS_RING } from '#tv/features/playback/player/playerStyles';
import { useDirectPlayback } from '#tv/features/playback/player/useDirectPlayback';
import { usePlayerControls } from '#tv/features/playback/player/usePlayerControls';
import { useStoryboard } from '#tv/features/playback/player/useStoryboard';
import { useSubtitleGen } from '#tv/features/playback/player/useSubtitleGen';
import { useSubtitleSelection } from '#tv/features/playback/player/useSubtitleSelection';
import { TvSubtitles } from '#tv/features/playback/TvSubtitles';

/** No credits marker → assume the last `CREDITS_TAIL`s are the credits. */
const CREDITS_TAIL = 30;
/** Scrub-preview thumbnail width (px); height follows the sheet's 16:9 tiles. */
const PREVIEW_W = 256;
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
  // Scrub-bar preview thumbnails (storyboard sprite sheet, generated server-side).
  const storyboard = useStoryboard(client, item.id);
  // On-device subtitle generation (Whisper transcribe / LLM translate): caps, the
  // live progress poll, the generate-sheet form, and remote field handlers. A
  // finished generation reloads the subtitle list (subs.reload).
  const gen = useSubtitleGen(client, item, subs.rendered, subs.reload);
  const genRows = useMemo(() => gen.pending.map((g) => g.id), [gen.pending]);
  const canCreate = Boolean(gen.caps?.transcribe || gen.caps?.translate);
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
    // swap, not replace: keep the history below so Back returns to the show/detail
    // you launched from instead of dead-ending on this lone player.
    nav.swap('player', { item: next });
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
    if (intro) playback.seekTo(intro.endMs / 1000);
  }, [intro, playback.seekTo]);

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
    genOpen,
    genFocus,
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
    terminated: terminated != null,
    audioOptions,
    activeAudio: playback.audioIndex,
    pickAudio: playback.setAudio,
    subOptions: subs.options,
    pickSub: subs.pick,
    genRows,
    onCancelGen: gen.cancel,
    canCreate,
    genFields: gen.fields,
    hasNext: Boolean(next),
    onNext: goNext,
    canSkipIntro,
    onSkipIntro: skipIntro,
    upNext: showUpNext,
    onUpNextPlay: goNext,
    onUpNextCancel: cancelUpNext,
  });

  const audio = useMemo(() => audioSupport(item), [item]);
  const subtitle = playerSubtitle(item);

  // While progressively seeking, the bar + time preview the pending position.
  const shown = seekPreview ?? cur;
  const pct = dur ? (shown / dur) * 100 : 0;
  const bufPct = dur ? (bufEnd / dur) * 100 : 0;
  // Show the storyboard thumbnail whenever the progress bar is the active focus
  // (the user is scrubbing); it tracks the previewed position.
  const previewTile = controls && zone === 'progress' ? storyboard.tile(shown, PREVIEW_W) : null;

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

  // Auto-advance fallback when playback actually reaches the end (seeking near the
  // end must NOT teleport to the next episode the countdown above is the primary
  // path). Driven by the engine's ended signal so it works for AVPlay too.
  useEffect(() => {
    if (playback.endedNonce > 0 && next && !upNextCancelled) goNext();
  }, [playback.endedNonce, next, upNextCancelled, goNext]);

  const endsAt = dur ? endsAtClock(Math.max(0, dur - cur) * 1000, locale) : '';
  const fade = controls ? 'opacity-100' : 'pointer-events-none opacity-0';
  // Warning pill text, by priority: stream/codec load error → direct-play verdict
  // → audio support. (An admin stop gets its own blocking overlay below, not the
  // pill.) All resolved to the active locale here.
  let warn: string | null = null;
  if (error) warn = t(error);
  else if (verdict && !verdict.canDirectPlay) warn = t(verdict.messageKey, verdict.messageVars);
  else if (!audio.canPlay && audio.messageKey) warn = t(audio.messageKey, audio.messageVars);

  return (
    <div
      className={`fixed inset-0 z-60 ${playback.surface === 'avplay' ? 'bg-transparent' : 'bg-black'} ${controls ? '' : 'cursor-none'}`}
    >
      {playback.surface === 'avplay' ? (
        // Native AVPlay renders to a hardware video plane BEHIND the page; this
        // <object> is the placeholder surface and the page is transparent here so
        // the plane shows through. The HTML chrome + subtitles sit on top.
        <object
          ref={playback.objectRef}
          type="application/avplayer"
          className="h-full w-full"
          aria-label={item.title}
        />
      ) : (
        // eslint-disable-next-line jsx-a11y/media-has-caption
        <video
          ref={playback.videoRef}
          className="h-full w-full bg-black object-contain"
          autoPlay
          playsInline
        />
      )}
      <TvSubtitles
        positionSec={cur}
        playing={playing && !waiting}
        seekNonce={playback.seekNonce}
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
        previewTile={previewTile}
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
          pending={gen.pending}
          canCreate={canCreate}
          genOpen={genOpen}
          genFocus={genFocus}
          form={gen.form}
        />
      ) : null}

      {/* Admin stopped this stream: a blocking overlay that locks the transport
          (usePlayerControls swallows every key) and exits on OK / Retour, so the
          viewer can't silently resume an untracked stream. Mirrors the web modal. */}
      {terminated != null ? (
        <div className="absolute inset-0 z-80 flex flex-col items-center justify-center gap-6 bg-[rgba(0,0,0,0.92)] px-16 text-center backdrop-blur-sm">
          <span className="text-[#E8536A]">
            <StopGlyph size={64} />
          </span>
          <div className="font-display text-[30px] font-bold text-white">
            {t('player.stoppedTitle')}
          </div>
          <p className="max-w-[42rem] font-sans text-[18px] leading-relaxed text-[rgba(244,243,240,0.72)]">
            {terminated || t('player.stoppedDefault')}
          </p>
          <div
            className={`mt-2 flex items-center gap-2 rounded-full bg-accent px-7 py-3 font-sans text-[16px] font-bold text-accent-ink ${FOCUS_RING}`}
          >
            <BackChevron />
            {t('player.back')}
          </div>
        </div>
      ) : null}
    </div>
  );
}
