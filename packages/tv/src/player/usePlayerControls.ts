import { dispatchRemoteKey, type RemoteKeyMap, resolveRemoteKey } from '@luma/core';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

export type Zone = 'progress' | 'bar';
/** Bottom control row, left → right. */
export const BAR = ['rewind', 'play', 'forward', 'av'] as const;

interface Args {
  playing: boolean;
  togglePlay: () => void;
  seek: (delta: number) => void;
  onExit: () => void;
  /** Audio-track indices, in panel order. */
  audioOptions: number[];
  activeAudio: number;
  pickAudio: (index: number) => void;
  subOptions: (number | null)[];
  pickSub: (index: number | null) => void;
}

/** One selectable row in the Audio & Subtitles panel — audio rows first, then
 * subtitle rows — so a single focus index walks both sections. */
type AvRow = { kind: 'audio'; value: number } | { kind: 'sub'; value: number | null };

export interface PlayerControls {
  controls: boolean;
  zone: Zone;
  barIndex: number;
  avOpen: boolean;
  avFocus: number;
  /** Is bottom-bar control `i` the focused one (and controls visible)? */
  barFocus: (i: number) => boolean;
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
  onExit,
  audioOptions,
  activeAudio,
  pickAudio,
  subOptions,
  pickSub,
}: Args): PlayerControls {
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
      switch (BAR[i]) {
        case 'rewind':
          seek(-10);
          break;
        case 'play':
          togglePlay();
          break;
        case 'forward':
          seek(10);
          break;
        case 'av':
          openAv();
          break;
      }
    },
    [seek, togglePlay, openAv],
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
      if (zone === 'progress') seek(dir * 10);
      else setBarIndex((i) => Math.max(0, Math.min(BAR.length - 1, i + dir)));
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

    const onKey = (e: KeyboardEvent) => {
      const key = resolveRemoteKey(e);
      if (!key) return;
      // Ignore auto-repeat for discrete OK actions — a held OK that entered the
      // player must not immediately toggle playback or re-trigger a control.
      if (e.repeat && (key === 'Enter' || key === 'PlayPause')) {
        e.preventDefault();
        return;
      }

      if (avOpen) {
        dispatchRemoteKey(e, avMap);
        return;
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
        FastForward: () => seek(30),
        Rewind: () => seek(-10),
      });
      e.preventDefault(); // the visible overlay swallows every key
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    avOpen,
    avFocus,
    avRows,
    controls,
    zone,
    barIndex,
    playing,
    onExit,
    poke,
    seek,
    togglePlay,
    activate,
    pickAudio,
    pickSub,
  ]);

  const barFocus = (i: number) => controls && zone === 'bar' && barIndex === i;
  return { controls, zone, barIndex, avOpen, avFocus, barFocus };
}
