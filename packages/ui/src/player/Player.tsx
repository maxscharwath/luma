import { formatTimecode as fmtTime, type Marker, type RemoteKey } from '@kroma/core';
import {
  type CSSProperties,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useLocale, useT } from '../i18n';
import type { StoryboardTile } from '../storyboard';
import { ChapterProgressBar } from './ChapterProgressBar';
import { ControlCluster } from './ControlCluster';
import { CreditsCard, type CreditsCardItem } from './CreditsCard';
import { currentChapter, normalizeChapters } from './chapters';
import { clamp01, endsAtClock, sliderToVolume, volumeToSlider } from './fmt';
import type { PanelHandle } from './nav';
import { SettingsPanel } from './SettingsPanel';
import { SkipIntroButton } from './SkipIntroButton';
import { StatsPanel } from './StatsPanel';
import { SubtitleRenderer } from './SubtitleRenderer';
import type { SubtitleGenBundle } from './settings/gen';
import { injectKeyframes } from './styles';
import type { SubtitleAppearance } from './subtitle-appearance';
import { TopBar } from './TopBar';
import type { Chapter, PlayerController, PlayerFlags } from './types';
import { type UpNextData, type UpNextItem, UpNextSheet } from './UpNextSheet';
import { usePlayerCredits } from './usePlayerCredits';
import { usePlayerKeys } from './usePlayerKeys';
import { usePlayerNav } from './usePlayerNav';

export interface PlayerProps {
  controller: PlayerController;
  flags: PlayerFlags;
  title: string;
  subtitle?: string;
  /** Already-localized warning pill (codec / audio support), or null. */
  warn?: string | null;
  /** Raw chapter data (normalized here). */
  chapters?: Chapter[];
  /** Intro / credits markers (drives skip-intro + credits autoplay). */
  markers?: readonly Marker[];
  /** Storyboard preview tile at a position (null until the sheet is ready). */
  tileAt: (sec: number) => StoryboardTile | null;
  appearance: SubtitleAppearance;
  onAppearance: (next: Partial<SubtitleAppearance>) => void;
  subtitleGen: SubtitleGenBundle;
  /** "À suivre" data (§10): next episodes + recommendations. */
  upNext: UpNextData;
  /** Play an up-next card (recommendation / next episode from the sheet). */
  onPlayItem?: (item: UpNextItem) => void;
  /** Next episode for the credits autoplay + the cluster next button (§11). */
  onPlayNext?: () => void;
  nextTitle?: CreditsCardItem | null;
  /** Skip-intro window (§13). */
  intro?: { active: boolean; onSkip: () => void };
  /** The video surface (an in-page <video> or a native-plane placeholder). */
  surface: ReactNode;
  /** Blocking admin-stop overlay (locks the transport when present). */
  terminated?: ReactNode;
  /** Floating toasts (resume prompt, etc.). */
  children?: ReactNode;
  /** The element that goes fullscreen on web (the player root). */
  rootRef?: React.Ref<HTMLDivElement>;
  onClose: () => void;
}

function initialSettingsView(overlay: string | null): 'audio' | 'subtitles' | 'menu' {
  if (overlay === 'audio') return 'audio';
  if (overlay === 'subtitles') return 'subtitles';
  return 'menu';
}

// The stage scales/translates (transform-origin + transform) for a smooth shrink
// into a rounded card on the left when the settings panel opens. Native TV planes
// can't be transformed, so those never shrink (settingsShrink stays false). PiP is
// the browser's own floating window (web), so it needs no in-page transform.
function stageTransformFor(settingsShrink: boolean): CSSProperties {
  if (settingsShrink) {
    return {
      transformOrigin: '0 50%',
      transform: 'translate(3vw,0) scale(0.5)',
      // The card is drawn at scale 0.5, so the on-screen radius is half this.
      borderRadius: 48,
    };
  }
  return { transformOrigin: '0 50%', transform: 'none', borderRadius: 0 };
}

/** Derived chrome-visibility flags, kept out of the component to stay flat. The
 * video only shrinks into a card for an IN-PAGE surface; native planes just get
 * the panel slid over them. */
function deriveChrome(
  nav: ReturnType<typeof usePlayerNav>,
  c: PlayerController,
  props: Readonly<PlayerProps>,
) {
  const settingsOpen =
    nav.overlay === 'settings' || nav.overlay === 'audio' || nav.overlay === 'subtitles';
  const sheetOpen = nav.overlay === 'sheet';
  const settingsShrink = settingsOpen && c.surface === 'video';
  const hasUpNext = props.upNext.nextEpisodes.length + props.upNext.recommendations.length > 0;
  const peekVisible = nav.revealed && hasUpNext && !settingsShrink && !nav.overlay;
  const chromeShown = nav.revealed && !nav.overlay;
  return { settingsOpen, sheetOpen, settingsShrink, peekVisible, chromeShown };
}

/** Credits card key routing: Left/Right swap Play/Cancel focus, OK fires the
 * focused one, Back cancels. Returns whether the key was consumed. */
function handleCreditsKey(
  key: RemoteKey,
  focus: 'play' | 'cancel',
  setFocus: Dispatch<SetStateAction<'play' | 'cancel'>>,
  onPlay: () => void,
  onCancel: () => void,
): boolean {
  if (key === 'Left' || key === 'Right') {
    setFocus((f) => (f === 'play' ? 'cancel' : 'play'));
    return true;
  }
  if (key === 'Enter') {
    if (focus === 'play') onPlay();
    else onCancel();
    return true;
  }
  if (key === 'Back') {
    onCancel();
    return true;
  }
  return false;
}

/** Pointer + keyboard handlers for the player root / stage, hoisted out of the
 * component so its cognitive complexity stays low. The stage click/key pair is a
 * pointer convenience (toggle play, double-click fullscreen); D-pad control still
 * flows through usePlayerKeys, so the key handler stops propagation. */
function playerInputHandlers(
  nav: ReturnType<typeof usePlayerNav>,
  c: PlayerController,
  flags: PlayerFlags,
  locked: boolean,
) {
  return {
    onPointerMove: (e: React.PointerEvent) => {
      if (e.pointerType !== 'touch') nav.poke();
    },
    onStageClick: () => {
      if (!locked) {
        nav.poke();
        c.togglePlay();
      }
    },
    onStageKeyDown: (e: React.KeyboardEvent) => {
      if ((e.key === 'Enter' || e.key === ' ') && !locked) {
        e.preventDefault();
        e.stopPropagation();
        nav.poke();
        c.togglePlay();
      }
    },
    onStageDoubleClick: () => {
      if (flags.fullscreen) c.toggleFullscreen();
    },
  };
}

/**
 * The unified player chrome (§14): one component for web + TV. It owns the nav
 * machine, the keyboard router, the credits autoplay and the settings / PiP
 * video transforms, and composes every surface (top bar, chapter bar, control
 * cluster, settings panel, up-next sheet, subtitle renderer, overlays). The
 * platform provides a {@link PlayerController} + feature flags; nothing here
 * talks to an engine directly.
 */
export function Player(props: Readonly<PlayerProps>) {
  useEffect(injectKeyframes, []);
  const { controller: c, flags } = props;
  const t = useT();
  const locale = useLocale();

  const [statsOn, setStatsOn] = useState(false);
  const panelRef = useRef<PanelHandle>(null);
  const locked = Boolean(props.terminated);

  const chapters = useMemo(
    () => normalizeChapters(props.chapters, c.dur * 1000),
    [props.chapters, c.dur],
  );
  const shown = c.seekPreview ?? c.cur;
  const curChapter = currentChapter(chapters, shown * 1000);

  const credits = usePlayerCredits({
    markers: props.markers,
    dur: c.dur,
    cur: c.cur,
    seeking: c.seekPreview != null,
    endedNonce: c.endedNonce,
    hasNext: Boolean(props.onPlayNext),
    onAdvance: () => props.onPlayNext?.(),
  });
  const [creditsFocus, setCreditsFocus] = useState<'play' | 'cancel'>('play');
  useEffect(() => {
    if (credits.show) setCreditsFocus('play');
  }, [credits.show]);

  const nav = usePlayerNav(flags, c.playing, {
    togglePlay: c.togglePlay,
    seekNudge: (d) => c.skip(d * 10),
    onNext: () => props.onPlayNext?.(),
    hasNext: Boolean(props.onPlayNext),
    // Step in perceptual slider space so a nudge feels even across the range.
    volumeNudge: (d) => c.setVolume(sliderToVolume(clamp01(volumeToSlider(c.volume) + d * 0.05))),
    toggleMute: c.toggleMute,
    togglePip: c.togglePip,
    toggleFullscreen: c.toggleFullscreen,
    onExit: props.onClose,
  });

  const creditsKey = (key: RemoteKey): boolean =>
    handleCreditsKey(
      key,
      creditsFocus,
      setCreditsFocus,
      () => props.onPlayNext?.(),
      credits.cancel,
    );

  usePlayerKeys({
    nav,
    controller: c,
    flags,
    panelRef,
    locked,
    intro: props.intro,
    credits: { active: credits.show, onKey: creditsKey },
  });

  const { settingsOpen, sheetOpen, settingsShrink, peekVisible, chromeShown } = deriveChrome(
    nav,
    c,
    props,
  );
  const initialView = initialSettingsView(nav.overlay);
  // Subtitles live inside the stage, so they scale WITH the video (stay in the
  // card, §5).
  const stage = stageTransformFor(settingsShrink);
  const endsAt = c.dur ? endsAtClock(Math.max(0, c.dur - c.cur) * 1000, locale) : '';
  // The top bar + transport hide while a panel / PiP owns the screen, and whenever
  // the chrome auto-hides.
  const chromeFade = chromeShown ? 'opacity-100' : 'pointer-events-none opacity-0';
  const input = playerInputHandlers(nav, c, flags, locked);

  return (
    <div
      ref={props.rootRef}
      className={`fixed inset-0 z-60 ${c.surface === 'video' ? 'bg-black' : 'bg-transparent'} ${nav.revealed ? '' : 'cursor-none'}`}
      onPointerMove={input.onPointerMove}
    >
      {/* stage: video + subtitles, transformed together to shrink into the
          settings card. role="button" (not a native <button>): it wraps the
          <video> surface + subtitles + spinner, which a button may not contain,
          and legacy-TV webviews render it more reliably. Keyboard parity via
          onStageKeyDown. */}
      {/* biome-ignore lint/a11y/useSemanticElements: a native <button> can't wrap the video/subtitle/spinner surface; keyboard parity is provided. */}
      <div
        role="button"
        tabIndex={0}
        aria-label={c.playing ? t('player.pause') : t('player.play')}
        className={`absolute inset-0 z-[2] overflow-hidden transition-[transform,border-radius,box-shadow] duration-[420ms] ease-[cubic-bezier(.22,1,.36,1)] ${settingsShrink ? 'bg-black shadow-pop [&>video]:object-cover' : 'bg-transparent [&>video]:object-contain'}`}
        style={stage}
        onClick={input.onStageClick}
        onKeyDown={input.onStageKeyDown}
        onDoubleClick={input.onStageDoubleClick}
      >
        {props.surface}
        <SubtitleRenderer
          positionSec={c.cur}
          playing={c.playing}
          subtitles={c.subtitles}
          activeIndex={c.subtitleIndex}
          appearance={props.appearance}
          raised={nav.revealed}
        />
        {/* Buffering spinner lives INSIDE the stage so it shrinks with the video
            into the settings card (not floating over the full page). */}
        {c.waiting && !locked ? (
          <div className="pointer-events-none absolute inset-0 z-[4] flex items-center justify-center">
            <div className="h-14 w-14 rounded-full border-[3px] border-[rgba(255,255,255,0.2)] border-t-accent [animation:kpl-spin_0.9s_linear_infinite]" />
          </div>
        ) : null}
      </div>

      {/* skip intro (§13) */}
      {props.intro ? (
        <SkipIntroButton
          visible={props.intro.active}
          focused={props.intro.active && !nav.overlay && !credits.show}
          onSkip={props.intro.onSkip}
        />
      ) : null}

      {/* credits autoplay (§11) */}
      {credits.show && props.nextTitle ? (
        <CreditsCard
          item={props.nextTitle}
          secondsLeft={credits.secondsLeft}
          total={credits.total}
          playFocused={creditsFocus === 'play'}
          cancelFocused={creditsFocus === 'cancel'}
          onPlay={() => props.onPlayNext?.()}
          onCancel={credits.cancel}
        />
      ) : null}

      {/* stats (§9) */}
      {statsOn ? <StatsPanel controller={c} onClose={() => setStatsOn(false)} /> : null}

      {/* top bar */}
      <div
        className={`absolute inset-x-0 top-0 z-20 transition-opacity duration-350 ${chromeFade}`}
      >
        <TopBar
          title={props.title}
          subtitle={props.subtitle}
          warn={props.warn}
          onBack={props.onClose}
        />
      </div>

      {/* up-next sheet (peek + expand, §10) */}
      <UpNextSheet
        ref={sheetOpen ? panelRef : null}
        data={props.upNext}
        open={sheetOpen}
        revealed={peekVisible || sheetOpen}
        onOpen={() => nav.openOverlay('sheet')}
        onClose={() => nav.closeOverlay()}
        onPlay={(item) => props.onPlayItem?.(item)}
      />

      {/* bottom chrome: chapter bar + control cluster. The gradient stays anchored
          to the screen bottom (never floated up), and the controls are lifted
          above the up-next peek with padding instead - so the peek (higher
          z-index) overlays the gradient's dark foot seamlessly rather than the
          gradient ending in a hard shadow band just above the peek. */}
      <div
        className={`absolute inset-x-0 bottom-0 z-[15] bg-[linear-gradient(0deg,rgba(0,0,0,0.82),transparent)] px-[34px] pt-20 transition-[padding,opacity] duration-300 ${chromeFade}`}
        style={{ paddingBottom: peekVisible ? 146 : 28 }}
      >
        <ChapterProgressBar
          cur={c.cur}
          dur={c.dur}
          bufEnd={c.bufEnd}
          seekPreview={c.seekPreview}
          chapters={chapters}
          tileAt={props.tileAt}
          focused={nav.zone === 'progress'}
          elapsed={fmtTime(shown)}
          chapterLabel={curChapter?.title || undefined}
          total={fmtTime(c.dur)}
          endsAt={endsAt ? t('content.endsAtShort', { time: endsAt }) : ''}
          onScrub={c.scrubPreview}
          onScrubCommit={c.scrubCommit}
        />
        <ControlCluster
          controls={nav.controls}
          focused={nav.focusedControl}
          playing={c.playing}
          muted={c.muted}
          volume={c.volume}
          pipActive={c.pipActive}
          fullscreen={c.fullscreen}
          onActivate={nav.activate}
          onFocus={nav.focusControl}
          onVolume={c.setVolume}
        />
      </div>

      {/* settings / audio / subtitles panel (§5) */}
      {settingsOpen ? (
        <SettingsPanel
          ref={panelRef}
          initialView={initialView}
          controller={c}
          appearance={props.appearance}
          onAppearance={props.onAppearance}
          statsOn={statsOn}
          onToggleStats={() => setStatsOn((s) => !s)}
          subtitleGen={props.subtitleGen}
          onClose={() => nav.closeOverlay()}
        />
      ) : null}

      {props.terminated}
      {props.children}
    </div>
  );
}
