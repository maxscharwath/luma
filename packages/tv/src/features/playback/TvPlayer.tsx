import { audioSupport, type MediaItem, playerSubtitle, type Translate } from '@kroma/core';
import { Player, TV_FLAGS, type UpNextItem, useSubtitleAppearance, useT } from '@kroma/ui';
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useEnv } from '#tv/app/providers/env';
import { useClient, useNav, useParams } from '#tv/app/router';
import { getTauri } from '#tv/features/playback/player/engine';
import { BackChevron, StopGlyph } from '#tv/features/playback/player/icons';
import { FOCUS_RING } from '#tv/features/playback/player/playerStyles';
import type { Playback } from '#tv/features/playback/player/useDirectPlayback';
import { useNowPlaying } from '#tv/features/playback/player/useNowPlaying';
import { useStoryboard } from '#tv/features/playback/player/useStoryboard';
import { useTvController } from '#tv/features/playback/use-tv-controller';
import { useTvUpNext } from '#tv/features/playback/use-tv-upnext';

/** Scrub-preview thumbnail width (px); the storyboard tile keeps 16:9. */
const PREVIEW_W = 256;

/** Warning pill text, by priority: stream/codec error -> direct-play verdict
 * (in-page surface only) -> audio support. Null when nothing to warn about. */
function playerWarn(pb: Playback, item: MediaItem, t: Translate): string | null {
  if (pb.error) return t(pb.error);
  if (pb.surface === 'video' && pb.verdict && !pb.verdict.canDirectPlay)
    return t(pb.verdict.messageKey, pb.verdict.messageVars);
  const audio = audioSupport(item);
  if (!audio.canPlay && audio.messageKey) return t(audio.messageKey, audio.messageVars);
  return null;
}

/**
 * The TV player: a thin wrapper adapting the native-plane engine to the shared
 * unified `<Player>` (packages/ui/src/player), with TV feature flags (no volume /
 * PiP / fullscreen). All chrome + D-pad interaction live in the shared component;
 * this handles the surface plane, the "up next" series autoplay and the OS
 * now-playing widget.
 */
export function TvPlayer() {
  const nav = useNav();
  const { item } = useParams('player');
  const client = useClient();
  const t = useT();
  // Reveal-on-pointer only in the DESKTOP (Tauri) shell, where a real mouse
  // exists. Real TVs (Tizen / webOS / Android TV) have no Tauri and their
  // magic-remote cursor reports `(pointer: fine)` while emitting phantom
  // pointermove events - which would pin the chrome (title + controls) open
  // forever. There the D-pad drives reveal, so the chrome auto-hides on idle.
  const { pointer } = useEnv();
  const playerFlags = useMemo(
    () => ({ ...TV_FLAGS, pointer: pointer && getTauri() != null }),
    [pointer],
  );

  const { controller, pb, subtitleGen } = useTvController(client, item);
  const [appearance, setAppearance] = useSubtitleAppearance();
  const storyboard = useStoryboard(client, item.id);
  const tileAt = useCallback((sec: number) => storyboard.tile(sec, PREVIEW_W), [storyboard]);

  // Upcoming episodes (series autoplay uses [0]) + the up-next sheet data.
  const [following, setFollowing] = useState<MediaItem[]>([]);
  const advancedRef = useRef(false);
  useEffect(() => {
    advancedRef.current = false;
    setFollowing([]);
    let cancelled = false;
    client
      .followingEpisodes(item.id)
      .then((list) => !cancelled && setFollowing(list))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, item.id]);
  const next = following[0] ?? null;
  const up = useTvUpNext(client, item, following);

  const goNext = useCallback(() => {
    if (advancedRef.current || !next) return;
    advancedRef.current = true;
    // swap, not push: Back returns to the show/detail you launched from.
    nav.swap('player', { item: next });
  }, [next, nav]);
  const onPlayItem = useCallback(
    (i: UpNextItem) => {
      const full = up.byId.get(i.id);
      if (full) nav.swap('player', { item: full });
    },
    [up.byId, nav],
  );

  const subtitle = playerSubtitle(item);
  useNowPlaying({
    client,
    item,
    title: item.title,
    subtitle,
    durationSec: pb.dur,
    positionSec: pb.cur,
    playing: pb.playing,
    seekTo: pb.seekTo,
  });

  // Intro window (episodes only).
  const intro = useMemo(() => (item.markers ?? []).find((m) => m.kind === 'intro'), [item.markers]);
  const introActive =
    intro != null && pb.cur * 1000 >= intro.startMs && pb.cur * 1000 < intro.endMs;

  // Native planes (mpv / ExoPlayer / AVPlay) render behind the page, so it must be
  // transparent once a fresh frame is up (kept opaque while loading).
  useEffect(() => {
    const native = pb.surface !== 'video';
    if (!native || !pb.ready || typeof document === 'undefined') return;
    const el = document.documentElement;
    el.classList.add('kroma-native-surface');
    return () => el.classList.remove('kroma-native-surface');
  }, [pb.surface, pb.ready]);

  const warn = playerWarn(pb, item, t);

  const nextTitle = next
    ? {
        title: next.episodeTitle ?? next.title,
        subtitle:
          next.season != null && next.episode != null
            ? `S${next.season} E${next.episode}`
            : undefined,
        posterUrl: client.backdropFor(next) ?? client.posterFor(next),
      }
    : null;

  let surface: ReactNode;
  if (pb.surface === 'avplay') {
    // NO child text: AVPlay renders the video to a hardware plane, not into this
    // <object>'s box, so any fallback children (e.g. the title) would render
    // VISIBLY over the plane - a static title stuck top-left on every file.
    // aria-label carries the accessible name without drawing anything.
    surface = (
      <object
        ref={pb.objectRef}
        type="application/avplayer"
        style={{ width: '100%', height: '100%' }}
        aria-label={item.title}
      />
    );
  } else if (pb.surface === 'mpv' || pb.surface === 'exo') {
    surface = <div style={{ width: '100%', height: '100%' }} role="img" aria-label={item.title} />;
  } else {
    surface = (
      // Subtitles render via the shared SubtitleRenderer; the empty captions track
      // is only present to satisfy the media-caption accessibility requirement.
      <video
        ref={pb.videoRef}
        // object-fit comes from the stage (contain normally, cover when it shrinks
        // into the settings card); borderRadius: inherit so the video clips itself
        // to the card's rounded corners (a transformed parent's overflow-hidden
        // doesn't clip a child <video>). Same fix as the web player's surface.
        style={{ width: '100%', height: '100%', background: '#000', borderRadius: 'inherit' }}
        autoPlay
        playsInline
      >
        <track kind="captions" />
      </video>
    );
  }

  return (
    <Player
      controller={controller}
      flags={playerFlags}
      title={item.title}
      subtitle={subtitle}
      warn={warn}
      markers={item.markers ?? undefined}
      tileAt={tileAt}
      appearance={appearance}
      onAppearance={setAppearance}
      subtitleGen={subtitleGen}
      upNext={up.data}
      onPlayItem={onPlayItem}
      onPlayNext={next ? goNext : undefined}
      nextTitle={nextTitle}
      intro={
        intro ? { active: introActive, onSkip: () => pb.seekTo(intro.endMs / 1000) } : undefined
      }
      surface={surface}
      onClose={nav.back}
      terminated={
        pb.terminated != null ? (
          <div className="absolute inset-0 z-80 flex flex-col items-center justify-center gap-6 bg-[rgba(0,0,0,0.92)] px-16 text-center backdrop-blur-sm">
            <span className="text-[#E8536A]">
              <StopGlyph size={64} />
            </span>
            <div className="font-display text-[30px] font-bold text-white">
              {t('player.stoppedTitle')}
            </div>
            <p className="max-w-[42rem] font-sans text-[18px] leading-relaxed text-[rgba(244,243,240,0.72)]">
              {pb.terminated || t('player.stoppedDefault')}
            </p>
            <button
              type="button"
              onClick={nav.back}
              className={`mt-2 flex cursor-pointer items-center gap-2 rounded-full bg-accent px-7 py-3 font-sans text-[16px] font-bold text-accent-ink outline-none ${FOCUS_RING}`}
            >
              <BackChevron />
              {t('player.back')}
            </button>
          </div>
        ) : null
      }
    />
  );
}
