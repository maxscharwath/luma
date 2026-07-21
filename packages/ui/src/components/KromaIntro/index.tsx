import { useCallback, useEffect, useRef, useState } from 'react';
import { CssIntro } from './CssIntro';
import { SAFETY_MS, VIDEO_SOURCES } from './constants';
import { IntroShell } from './IntroShell';
import { useIntroExit } from './useIntroExit';
import { useIntroKeys } from './useIntroKeys';

/**
 * KROMA cinematic brand intro: the bundled intro film (a chromatic light tunnel
 * that lands on the KR-wheel-MA lockup), full-screen with sound, shared by every
 * client. Ships as 4K60 HEVC only every target TV, Android TV, Apple device
 * and HW-decode Chrome plays it; anything without an HEVC decoder (decode/load
 * failure, no supported codec) falls back to the pure-CSS scene in
 * {@link CssIntro}.
 *
 * Browsers block autoplay-with-sound until a user gesture, so playback is tried
 * with sound first and falls back to muted; the first pointer/key interaction
 * then unmutes the film in place, and only rewinds it when it has barely
 * started (so an opening gesture still gets picture and sound together, while a
 * stray click or remote key mid-film cannot restart the intro). There are no
 * on-screen controls it auto-ends with the film, any key / remote button (OK,
 * Back, Space) skips, and a safety timer (armed from the video's own duration
 * once metadata loads) guarantees the intro ends even if playback stalls.
 *
 * Framework-free (plain inline styles) so it renders identically on the web SSR
 * shell and on old TV webviews. Mount as a full-screen overlay; `onDone` hands
 * off to the app.
 */
export interface KromaIntroProps {
  /** Called once the intro has finished (video ended or skipped). */
  onDone: () => void;
  /** Single-source override for the intro film. Defaults to the bundled
   * 4K60 HEVC film. */
  videoSrc?: string;
  /** Loop forever instead of ending (preview/idle-screen use). */
  loop?: boolean;
  /** Optional tagline overlaid during the film's final seconds. None by default:
   * the film ends on the bare lockup. */
  tagline?: string;
  /** Performance mode for weak TV GPUs. The video path decodes in hardware and
   * ignores it; the CSS fallback uses it to stay compositor-only. */
  lite?: boolean;
}

/** A solid #0A0A0C poster (2x2 PNG). Without a poster the WebView paints its own
 * placeholder for an un-started <video> - on Android TV a light panel with a
 * centre play glyph, which flashes for a few frames before the film decodes.
 * A matching-black poster keeps the stage black through the load instead. */
const POSTER =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR42mPg4uIBIgYIBQADhgCBD73RIwAAAABJRU5ErkJggg==';

/** Seconds before the end of the film when the tagline overlay fades in. */
const TAGLINE_LEAD_S = 2.6;
/** Slack added to the video duration for the stall-safety timer (ms). */
const SAFETY_SLACK_MS = 1500;
/** How far into the film a first gesture still rewinds it to play with sound
 * from the top. Past this the gesture only unmutes. */
const UNMUTE_REWIND_S = 0.4;

export function KromaIntro(props: Readonly<KromaIntroProps>) {
  const [videoFailed, setVideoFailed] = useState(false);
  if (videoFailed) {
    const { onDone, loop, tagline, lite } = props;
    return <CssIntro onDone={onDone} loop={loop} tagline={tagline} lite={lite} />;
  }
  return <VideoIntro {...props} onVideoError={() => setVideoFailed(true)} />;
}

function VideoIntro({
  onDone,
  videoSrc,
  loop = false,
  tagline,
  onVideoError,
}: Readonly<KromaIntroProps & { onVideoError: () => void }>) {
  const [tagVisible, setTagVisible] = useState(false);
  const { exiting, exitedRef, safetyRef, exit, reopen, clearTimers } = useIntroExit(onDone);

  const videoRef = useRef<HTMLVideoElement | null>(null);
  const loopRef = useRef(loop);
  loopRef.current = loop;
  // Latest callback without re-running the mount effect (avoids restarting the film).
  const onVideoErrorRef = useRef(onVideoError);
  onVideoErrorRef.current = onVideoError;

  // (Re-)arm the stall-safety timer from the film's real length when known.
  const armSafety = useCallback(() => {
    clearTimeout(safetyRef.current);
    if (loopRef.current) return;
    const d = videoRef.current?.duration;
    const ms = d && Number.isFinite(d) && d > 0 ? d * 1000 + SAFETY_SLACK_MS : SAFETY_MS;
    safetyRef.current = setTimeout(() => {
      // A hidden tab defers the media fetch entirely; don't burn the intro
      // while parked in the background, re-check once per safety window.
      const vv = videoRef.current;
      if (document.hidden && vv?.readyState === 0) {
        armSafetyRef.current();
        return;
      }
      exit();
    }, ms);
  }, [exit, safetyRef]);
  const armSafetyRef = useRef(armSafety);
  armSafetyRef.current = armSafety;

  const replay = useCallback(() => {
    const v = videoRef.current;
    if (!v) return;
    reopen();
    setTagVisible(false);
    try {
      v.currentTime = 0;
    } catch {
      /* not yet seekable harmless */
    }
    void v.play().catch(() => undefined);
    armSafety();
  }, [armSafety, reopen]);

  // First gesture while the film is muted: add sound in place. Chrome keeps the
  // whole film muted until then, so a rewind here would restart the intro on any
  // click or non-skip remote key; only a gesture at the very top rewinds, which
  // is what makes picture and sound open together when the user is early.
  const unblock = useCallback(() => {
    const v = videoRef.current;
    if (!v?.muted || exitedRef.current) return;
    v.muted = false;
    if (v.currentTime >= UNMUTE_REWIND_S) return;
    try {
      v.currentTime = 0;
    } catch {
      /* harmless */
    }
    void v.play().catch(() => undefined);
    armSafety();
  }, [armSafety, exitedRef]);

  useIntroKeys({ exit, replay, unblock });

  // biome-ignore lint/correctness/useExhaustiveDependencies: mount-only intro timeline; exit/armSafety/replay are stable useCallbacks and are intentionally omitted so the effect never re-arms (which would restart the film) on unrelated re-renders.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;

    // A failure landing after the user skipped is ignored: swapping in the CSS
    // scene would unmount us, and this cleanup would drop the pending hand-off,
    // so the fallback would play a whole second intro from the top.
    const fail = () => {
      if (!exitedRef.current) onVideoErrorRef.current();
    };

    // Sound-first autoplay; muted fallback when the browser blocks it. A
    // muted-too rejection means playback is genuinely broken: use the CSS scene.
    const begin = () => {
      const p = v.play();
      if (typeof p?.then === 'function') {
        p.catch(() => {
          v.muted = true;
          const p2 = v.play();
          if (typeof p2?.then === 'function') p2.catch(fail);
        });
      }
      armSafety();
    };

    // Chrome defers media loading in background tabs, which would stall the film
    // until the safety timer silently burned the once-per-session intro. If the
    // page opens hidden, hold everything until it first becomes visible.
    let pendingVisible = false;
    const onVisible = () => {
      if (pendingVisible && !document.hidden) {
        pendingVisible = false;
        document.removeEventListener('visibilitychange', onVisible);
        begin();
      }
    };
    if (document.hidden) {
      pendingVisible = true;
      document.addEventListener('visibilitychange', onVisible);
    } else {
      begin();
    }

    const onEnded = () => {
      if (loopRef.current) replay();
      else exit();
    };
    const onMeta = () => armSafety();
    v.addEventListener('ended', onEnded);
    v.addEventListener('loadedmetadata', onMeta);
    v.addEventListener('error', fail);

    return () => {
      clearTimers();
      document.removeEventListener('visibilitychange', onVisible);
      v.pause();
      v.removeEventListener('ended', onEnded);
      v.removeEventListener('loadedmetadata', onMeta);
      v.removeEventListener('error', fail);
    };
  }, []);

  // The tagline reveal is the film's only per-frame listener: wire it up only
  // when a tagline was actually asked for (no shell sets one today).
  useEffect(() => {
    const v = videoRef.current;
    if (!v || !tagline) return;
    const onTime = () => {
      if (v.duration && Number.isFinite(v.duration) && v.currentTime > v.duration - TAGLINE_LEAD_S)
        setTagVisible(true);
    };
    v.addEventListener('timeupdate', onTime);
    return () => v.removeEventListener('timeupdate', onTime);
  }, [tagline]);

  return (
    <IntroShell exiting={exiting}>
      {/* biome-ignore lint/a11y/useMediaCaption: decorative brand film with a musical sting only, no speech to caption; the shell carries the accessible name. */}
      <video
        ref={videoRef}
        playsInline
        preload="metadata"
        poster={POSTER}
        style={{
          position: 'absolute',
          inset: 0,
          width: '100%',
          height: '100%',
          objectFit: 'cover',
          background: '#0A0A0C',
        }}
      >
        {/* With no playable source (no HEVC decoder), play() rejects
            (NotSupportedError) on both attempts above and the CSS scene takes
            over. */}
        {videoSrc ? (
          <source src={videoSrc} />
        ) : (
          VIDEO_SOURCES.map((s) => <source key={s.src} src={s.src} type={s.type} />)
        )}
      </video>

      {/* tagline overlay during the film's landing */}
      {tagline && tagVisible ? (
        <div
          style={{
            position: 'absolute',
            left: 0,
            right: 0,
            bottom: '11%',
            textAlign: 'center',
            fontWeight: 700,
            fontSize: '1.8vmin',
            letterSpacing: '.42em',
            textTransform: 'uppercase',
            color: 'rgba(244,243,240,.52)',
            whiteSpace: 'nowrap',
            animation: 'kroma-tagIn .85s ease both',
            pointerEvents: 'none',
          }}
        >
          {tagline}
        </div>
      ) : null}
    </IntroShell>
  );
}
