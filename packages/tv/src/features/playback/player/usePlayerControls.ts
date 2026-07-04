import { dispatchRemoteKey, type RemoteKeyMap, resolveRemoteKey } from '@luma/core';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getTauri } from '#tv/features/playback/player/engine';
import { useOverlayFocus } from '#tv/features/playback/player/useOverlayFocus';
import type { GenField } from '#tv/features/playback/player/useSubtitleGen';

export type Zone = 'progress' | 'bar';
/** Bottom control row, left → right (a `next` control is inserted for series). */
export const BAR = ['rewind', 'play', 'forward', 'av'] as const;
const BAR_NEXT = ['rewind', 'play', 'forward', 'next', 'av'] as const;

interface Args {
  playing: boolean;
  togglePlay: () => void;
  /** Begin a directional seek press (short = stacking tap, held = accelerating scrub). */
  seekPress: (dir: -1 | 1) => void;
  /** A discrete directional tap (OK on the focused rewind/forward control). */
  seekTap: (dir: -1 | 1) => void;
  onExit: () => void;
  /** Admin stopped this stream: lock the transport (no resume/seek) and let any
   * OK/Retour just exit the player, mirroring the web client's blocking modal. */
  terminated?: boolean;
  /** Audio-track indices, in panel order. */
  audioOptions: number[];
  activeAudio: number;
  pickAudio: (index: number) => void;
  subOptions: (number | null)[];
  pickSub: (index: number | null) => void;
  /** Ids of running generations shown as cancelable rows after the subtitles. */
  genRows?: string[];
  /** Cancel a running generation (OK on its row). */
  onCancelGen?: (id: string) => void;
  /** Whether the "create a missing subtitle" row + generate sheet are available. */
  canCreate?: boolean;
  /** The generate sheet's focusable controls (for the current mode). */
  genFields?: GenField[];
  /** Series only: show a "next episode" control + handler. */
  hasNext?: boolean;
  onNext?: () => void;
  /** Intro window is open: auto-focus the floating "skip intro" button so OK skips. */
  canSkipIntro?: boolean;
  onSkipIntro?: () => void;
  /** Up-next card is showing (credits): captures nav onto its Play/Cancel buttons. */
  upNext?: boolean;
  onUpNextPlay?: () => void;
  onUpNextCancel?: () => void;
}

/** One selectable row in the Audio & Subtitles panel: audio rows, then subtitle
 * rows, then any running-generation (cancel) rows, then the "create" row a single
 * focus index walks them all. */
type AvRow =
  | { kind: 'audio'; value: number }
  | { kind: 'sub'; value: number | null }
  | { kind: 'genCancel'; value: string }
  | { kind: 'create' };

export interface PlayerControls {
  controls: boolean;
  zone: Zone;
  barIndex: number;
  avOpen: boolean;
  avFocus: number;
  /** The generate sheet is open (captures the remote). */
  genOpen: boolean;
  /** Focused control index within the generate sheet. */
  genFocus: number;
  /** Is bottom-bar control `i` the focused one (and controls visible)? */
  barFocus: (i: number) => boolean;
  /** Is the named bottom-bar control focused (robust to the dynamic layout)? */
  barFocusName: (name: string) => boolean;
  /** Floating "skip intro" button is the active focus (OK skips). */
  skipFocused: boolean;
  /** Up-next card's "Play now" / "Cancel" buttons are focused. */
  upNextPlayFocus: boolean;
  upNextCancelFocus: boolean;
  /** Reveal the controls + restart the auto-hide timer (mouse activity). */
  poke: () => void;
  /** Move the focus ring to a named bottom-bar control (mouse click). */
  focusBar: (name: string) => void;
  /** Move focus to the progress bar (mouse scrub). */
  focusProgress: () => void;
  /** Open the Audio & Subtitles panel (mouse click on the tracks pill). */
  openAv: () => void;
}

/**
 * Owns the 10-foot control state (auto-hiding overlay, focus zone/index, AV
 * panel) and drives it entirely from the remote: ◀▶ move between controls, ▲
 * jumps to the progress bar, OK activates, Retour quits / closes the panel.
 */
export function usePlayerControls({
  playing,
  togglePlay,
  seekPress,
  seekTap,
  onExit,
  terminated = false,
  audioOptions,
  activeAudio,
  pickAudio,
  subOptions,
  pickSub,
  genRows = [],
  onCancelGen,
  canCreate = false,
  genFields = [],
  hasNext = false,
  onNext,
  canSkipIntro = false,
  onSkipIntro,
  upNext = false,
  onUpNextPlay,
  onUpNextCancel,
}: Args): PlayerControls {
  // Active bottom-bar layout (a `next` control is inserted for series).
  const bar: readonly string[] = hasNext ? BAR_NEXT : BAR;
  // Combined, ordered row list the ▲/▼ focus walks: audio rows then subtitle rows.
  const avRows = useMemo<AvRow[]>(
    () => [
      ...audioOptions.map((value): AvRow => ({ kind: 'audio', value })),
      ...subOptions.map((value): AvRow => ({ kind: 'sub', value })),
      ...genRows.map((value): AvRow => ({ kind: 'genCancel', value })),
      ...(canCreate ? [{ kind: 'create' } as AvRow] : []),
    ],
    [audioOptions, subOptions, genRows, canCreate],
  );
  const [controls, setControls] = useState(true);
  const [zone, setZone] = useState<Zone>('bar');
  const [barIndex, setBarIndex] = useState(1); // start on Play
  const [avOpen, setAvOpen] = useState(false);
  const [avFocus, setAvFocus] = useState(0);
  // Generate sheet (opened from the "create" row): owns the remote while open.
  const [genOpen, setGenOpen] = useState(false);
  const [genFocus, setGenFocus] = useState(0);
  // Interrupt-overlay focus (skip-intro button + up-next card), independent of
  // the auto-hiding control bar.
  const { skipFocused, setSkipFocused, cardFocused, setCardFocused, upNextFocus, setUpNextFocus } =
    useOverlayFocus(canSkipIntro, upNext);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const poke = useCallback(() => {
    setControls(true);
    if (hideTimer.current) clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => {
      if (playing) setControls(false); // hide only while actively playing
    }, 3500);
  }, [playing]);

  // Keep controls visible while paused or the AV panel is open.
  useEffect(() => {
    if (!playing || avOpen) setControls(true);
    else poke();
    return () => {
      if (hideTimer.current) clearTimeout(hideTimer.current);
    };
  }, [playing, avOpen, poke]);

  const openAv = useCallback(() => {
    // Open focused on the active audio track (audio rows lead the list).
    const i = avRows.findIndex((r) => r.kind === 'audio' && r.value === activeAudio);
    setAvFocus(Math.max(0, i));
    setGenOpen(false);
    setAvOpen(true);
  }, [avRows, activeAudio]);

  // Mouse focus helpers: a pointer interaction moves the focus ring to match, so
  // the keyboard/remote resumes from wherever the mouse last acted.
  const focusBar = useCallback(
    (name: string) => {
      const i = bar.indexOf(name);
      if (i >= 0) {
        setZone('bar');
        setBarIndex(i);
      }
    },
    [bar],
  );
  const focusProgress = useCallback(() => setZone('progress'), []);

  const activate = useCallback(
    (i: number) => {
      switch (bar[i]) {
        case 'rewind':
          seekTap(-1);
          break;
        case 'play':
          togglePlay();
          break;
        case 'forward':
          seekTap(1);
          break;
        case 'next':
          onNext?.();
          break;
        case 'av':
          openAv();
          break;
      }
    },
    [bar, seekTap, togglePlay, openAv, onNext],
  );

  useEffect(() => {
    // Pick the focused row in the open AV panel (audio, subtitle, cancel-a-running
    // generation, or open the generate sheet).
    const pickRow = () => {
      const row = avRows[avFocus];
      if (row?.kind === 'audio') pickAudio(row.value);
      else if (row?.kind === 'sub') pickSub(row.value);
      else if (row?.kind === 'genCancel') onCancelGen?.(row.value);
      else if (row?.kind === 'create') {
        setGenFocus(0);
        setGenOpen(true);
      }
    };
    // While the overlay is hidden, the first key only reveals it (no action).
    const move = (reveal: boolean, dir: -1 | 1) => {
      if (reveal) return;
      if (zone === 'progress')
        seekPress(dir); // tap = 5s (stacks); hold = accelerating scrub
      else setBarIndex((i) => Math.max(0, Math.min(bar.length - 1, i + dir)));
    };
    const confirm = (reveal: boolean) => {
      if (reveal) return;
      if (zone === 'progress') togglePlay();
      else activate(barIndex);
    };

    // Audio/subtitle panel captures navigation while open (unbound keys ignored).
    const avMap: RemoteKeyMap = {
      Up: () => setAvFocus((f) => Math.max(0, f - 1)),
      Down: () => setAvFocus((f) => Math.min(avRows.length - 1, f + 1)),
      Enter: pickRow,
      PlayPause: pickRow,
      Back: () => setAvOpen(false),
      Stop: () => setAvOpen(false),
    };

    // Generate sheet: ▲▼ moves between controls, ◀▶ changes the focused control's
    // value, OK confirms (the "start" control closes the sheet), Retour closes it.
    const confirmGen = () => {
      const field = genFields[genFocus];
      field?.onEnter?.();
      if (field?.closeOnEnter) setGenOpen(false);
    };
    const genMap: RemoteKeyMap = {
      Up: () => setGenFocus((f) => Math.max(0, f - 1)),
      Down: () => setGenFocus((f) => Math.min(Math.max(0, genFields.length - 1), f + 1)),
      Left: () => genFields[genFocus]?.onLeft?.(),
      Right: () => genFields[genFocus]?.onRight?.(),
      Enter: confirmGen,
      PlayPause: confirmGen,
      Back: () => setGenOpen(false),
      Stop: () => setGenOpen(false),
    };

    // Up-next card captures nav onto its two buttons; Down drops back to the bar.
    const activateCard = () => {
      if (upNextFocus === 0) onUpNextPlay?.();
      else {
        onUpNextCancel?.();
        setCardFocused(false);
      }
    };
    const cardMap: RemoteKeyMap = {
      Left: () => setUpNextFocus(0),
      Right: () => setUpNextFocus(1),
      Enter: activateCard,
      PlayPause: activateCard,
      Down: () => setCardFocused(false),
      Back: onExit,
      Stop: onExit,
    };

    const onKey = (e: KeyboardEvent) => {
      const key = resolveRemoteKey(e);
      if (!key) return;
      // Ignore auto-repeat for discrete OK so a held OK never re-fires a control.
      if (e.repeat && (key === 'Enter' || key === 'PlayPause')) {
        e.preventDefault();
        return;
      }

      // Admin stopped the stream: the transport is locked behind a blocking
      // overlay. Swallow every key; OK / Retour just exit the player (so the
      // viewer can't silently resume an untracked stream).
      if (terminated) {
        e.preventDefault();
        if (key === 'Back' || key === 'Stop' || key === 'Enter' || key === 'PlayPause') onExit();
        return;
      }

      if (avOpen) {
        dispatchRemoteKey(e, genOpen ? genMap : avMap);
        return;
      }

      // Up-next card owns the remote while focused.
      if (cardFocused) {
        e.preventDefault();
        dispatchRemoteKey(e, cardMap);
        return;
      }

      // Dedicated transport keys (the remote's play/pause button, Space) toggle playback
      // DIRECTLY, independent of what control is focused - they never act as OK. Poke so
      // the change is visible.
      if (key === 'PlayPause' || key === 'Play' || key === 'Pause') {
        e.preventDefault();
        poke();
        if (key === 'Play') {
          if (!playing) togglePlay();
        } else if (key === 'Pause') {
          if (playing) togglePlay();
        } else {
          togglePlay();
        }
        return;
      }
      // Media next/prev (remote or keyboard ⏭/⏮): next episode / skip back.
      if (key === 'Next' || key === 'Prev') {
        e.preventDefault();
        poke();
        if (key === 'Next') onNext?.();
        else seekTap(-1);
        return;
      }

      // Skip-intro is auto-focused: OK skips; any direction hands focus to the bar.
      // (PlayPause is handled above, so it never reaches here.)
      if (skipFocused) {
        if (key === 'Enter') {
          e.preventDefault();
          onSkipIntro?.();
          setSkipFocused(false);
          return;
        }
        setSkipFocused(false);
      }

      // Back/Stop exits before the reveal/poke, so quitting never just wakes the bar.
      if (key === 'Back' || key === 'Stop') {
        e.preventDefault();
        onExit();
        return;
      }

      const reveal = !controls;
      poke();
      dispatchRemoteKey(e, {
        Up: () => setZone('progress'),
        Down: () => setZone('bar'),
        Left: () => move(reveal, -1),
        Right: () => move(reveal, 1),
        Enter: () => confirm(reveal),
        FastForward: () => seekPress(1),
        Rewind: () => seekPress(-1),
      });
      e.preventDefault(); // the visible overlay swallows every key
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    avOpen,
    avFocus,
    avRows,
    genOpen,
    genFocus,
    genFields,
    onCancelGen,
    bar,
    controls,
    zone,
    barIndex,
    playing,
    onExit,
    terminated,
    poke,
    seekPress,
    togglePlay,
    activate,
    pickAudio,
    pickSub,
    skipFocused,
    setSkipFocused,
    onSkipIntro,
    cardFocused,
    setCardFocused,
    upNextFocus,
    setUpNextFocus,
    onUpNextPlay,
    onUpNextCancel,
    onNext,
    seekTap,
  ]);

  // Current transport handlers + play state in a ref, so the media-session and media-key
  // subscriptions below register ONCE instead of tearing down + rebuilding on every toggle.
  const transportRef = useRef({ playing, togglePlay, onNext, seekTap, seekPress, poke });
  transportRef.current = { playing, togglePlay, onNext, seekTap, seekPress, poke };

  // Media Session: route the OS "Now Playing" widget + a MacBook's ⏯/⏭/⏮ media keys to
  // the player wherever the shell has an active media session (the web / `<video>`
  // players). The desktop mpv plane has no media element, so there Space + the on-screen
  // transport are the reliable path.
  useEffect(() => {
    const ms = navigator.mediaSession as MediaSession | undefined;
    if (!ms || typeof ms.setActionHandler !== 'function') return;
    const set = (action: MediaSessionAction, handler: (() => void) | null) => {
      try {
        ms.setActionHandler(action, handler);
      } catch {
        /* action unsupported by this engine */
      }
    };
    const t = () => transportRef.current;
    const handlers: Record<string, () => void> = {
      play: () => {
        if (!t().playing) t().togglePlay();
      },
      pause: () => {
        if (t().playing) t().togglePlay();
      },
      nexttrack: () => t().onNext?.(),
      previoustrack: () => t().seekTap(-1),
      seekforward: () => t().seekPress(1),
      seekbackward: () => t().seekPress(-1),
    };
    for (const [a, h] of Object.entries(handlers)) set(a as MediaSessionAction, h);
    return () => {
      for (const a of Object.keys(handlers)) set(a as MediaSessionAction, null);
    };
  }, []);

  // Native MacBook media keys: the Rust shell registers MPRemoteCommandCenter and re-emits
  // each press as a `media-key` event (the desktop mpv plane has no media element for the
  // browser Media Session above). Route them to the same actions.
  useEffect(() => {
    const bridge = getTauri();
    if (!bridge) return;
    let un: (() => void) | undefined;
    let dead = false;
    void bridge.event
      .listen('media-key', (e) => {
        const action = String((e as { payload?: unknown }).payload ?? '');
        const t = transportRef.current;
        t.poke();
        if (action === 'next') t.onNext?.();
        else if (action === 'prev') t.seekTap(-1);
        // play / pause / playpause: toggle for any - robust to the OS's playbackState
        // lagging the real state by a frame.
        else t.togglePlay();
      })
      .then((u) => {
        if (dead) u();
        else un = u;
      });
    return () => {
      dead = true;
      un?.();
    };
  }, []);

  const barFocus = (i: number) => controls && zone === 'bar' && barIndex === i;
  const barFocusName = (name: string) => controls && zone === 'bar' && bar[barIndex] === name;
  return {
    controls,
    zone,
    barIndex,
    avOpen,
    avFocus,
    genOpen,
    genFocus,
    barFocus,
    barFocusName,
    skipFocused,
    upNextPlayFocus: cardFocused && upNextFocus === 0,
    upNextCancelFocus: cardFocused && upNextFocus === 1,
    poke,
    focusBar,
    focusProgress,
    openAv,
  };
}
