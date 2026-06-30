import { dispatchRemoteKey, type RemoteKeyMap, resolveRemoteKey } from '@luma/core';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useOverlayFocus } from '#tv/features/playback/player/useOverlayFocus';

export type Zone = 'progress' | 'bar';
/** Bottom control row, left → right (a `next` control is inserted for series). */
export const BAR = ['rewind', 'play', 'forward', 'av'] as const;
const BAR_NEXT = ['rewind', 'play', 'forward', 'next', 'av'] as const;

interface Args {
  playing: boolean;
  togglePlay: () => void;
  seek: (delta: number) => void;
  /** Progressive (accelerating) seek in a direction for held direction keys. */
  nudge: (dir: -1 | 1) => void;
  onExit: () => void;
  /** Audio-track indices, in panel order. */
  audioOptions: number[];
  activeAudio: number;
  pickAudio: (index: number) => void;
  subOptions: (number | null)[];
  pickSub: (index: number | null) => void;
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

/** One selectable row in the Audio & Subtitles panel audio rows first, then
 * subtitle rows so a single focus index walks both sections. */
type AvRow = { kind: 'audio'; value: number } | { kind: 'sub'; value: number | null };

export interface PlayerControls {
  controls: boolean;
  zone: Zone;
  barIndex: number;
  avOpen: boolean;
  avFocus: number;
  /** Is bottom-bar control `i` the focused one (and controls visible)? */
  barFocus: (i: number) => boolean;
  /** Is the named bottom-bar control focused (robust to the dynamic layout)? */
  barFocusName: (name: string) => boolean;
  /** Floating "skip intro" button is the active focus (OK skips). */
  skipFocused: boolean;
  /** Up-next card's "Play now" / "Cancel" buttons are focused. */
  upNextPlayFocus: boolean;
  upNextCancelFocus: boolean;
}

/**
 * Owns the 10-foot control state (auto-hiding overlay, focus zone/index, AV
 * panel) and drives it entirely from the remote: ◀▶ move between controls, ▲
 * jumps to the progress bar, OK activates, Retour quits / closes the panel.
 */
export function usePlayerControls({
  playing,
  togglePlay,
  seek,
  nudge,
  onExit,
  audioOptions,
  activeAudio,
  pickAudio,
  subOptions,
  pickSub,
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
    ],
    [audioOptions, subOptions],
  );
  const [controls, setControls] = useState(true);
  const [zone, setZone] = useState<Zone>('bar');
  const [barIndex, setBarIndex] = useState(1); // start on Play
  const [avOpen, setAvOpen] = useState(false);
  const [avFocus, setAvFocus] = useState(0);
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
    setAvOpen(true);
  }, [avRows, activeAudio]);

  const activate = useCallback(
    (i: number) => {
      switch (bar[i]) {
        case 'rewind':
          seek(-10);
          break;
        case 'play':
          togglePlay();
          break;
        case 'forward':
          seek(10);
          break;
        case 'next':
          onNext?.();
          break;
        case 'av':
          openAv();
          break;
      }
    },
    [bar, seek, togglePlay, openAv, onNext],
  );

  useEffect(() => {
    // Pick the focused row in the open AV panel (audio rows lead, then subtitles).
    const pickRow = () => {
      const row = avRows[avFocus];
      if (row?.kind === 'audio') pickAudio(row.value);
      else if (row?.kind === 'sub') pickSub(row.value);
    };
    // While the overlay is hidden, the first key only reveals it (no action).
    const move = (reveal: boolean, dir: -1 | 1) => {
      if (reveal) return;
      if (zone === 'progress')
        nudge(dir); // accelerating seek (hold to go faster)
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

      if (avOpen) {
        dispatchRemoteKey(e, avMap);
        return;
      }

      // Up-next card owns the remote while focused.
      if (cardFocused) {
        e.preventDefault();
        dispatchRemoteKey(e, cardMap);
        return;
      }

      // Skip-intro is auto-focused: OK skips; any direction hands focus to the bar.
      if (skipFocused) {
        if (key === 'Enter' || key === 'PlayPause') {
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
        PlayPause: () => confirm(reveal),
        Play: () => {
          if (!playing) togglePlay();
        },
        Pause: () => {
          if (playing) togglePlay();
        },
        FastForward: () => nudge(1),
        Rewind: () => nudge(-1),
      });
      e.preventDefault(); // the visible overlay swallows every key
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    avOpen,
    avFocus,
    avRows,
    bar,
    controls,
    zone,
    barIndex,
    playing,
    onExit,
    poke,
    nudge,
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
  ]);

  const barFocus = (i: number) => controls && zone === 'bar' && barIndex === i;
  const barFocusName = (name: string) => controls && zone === 'bar' && bar[barIndex] === name;
  return {
    controls,
    zone,
    barIndex,
    avOpen,
    avFocus,
    barFocus,
    barFocusName,
    skipFocused,
    upNextPlayFocus: cardFocused && upNextFocus === 0,
    upNextCancelFocus: cardFocused && upNextFocus === 1,
  };
}
