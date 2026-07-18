import { type ReactNode, type PointerEvent as ReactPointerEvent, useCallback, useRef } from 'react';
import { useT } from '../i18n';
import { clamp01, sliderToVolume, volumeToSlider } from './fmt';
import {
  IconAudioTrack,
  IconBack10,
  IconFullscreen,
  IconFullscreenExit,
  IconFwd10,
  IconGear,
  IconMute,
  IconNext,
  IconPause,
  IconPip,
  IconPlay,
  IconSubtitles,
  IconVolHigh,
  IconVolLow,
} from './icons';
import type { ControlId } from './nav';
import { CTRL, CTRL_OFF, CTRL_ON, FOCUS_RING } from './tw';

const TRANSPORT: ReadonlySet<ControlId> = new Set<ControlId>(['rewind', 'play', 'forward']);
const CTRL_ON_FOCUS = `${CTRL_ON} ${FOCUS_RING}`;
const PLAY_ON_FOCUS = `bg-accent-hover ${FOCUS_RING}`;

export interface ControlClusterProps {
  controls: ControlId[];
  focused: ControlId | null;
  playing: boolean;
  muted: boolean;
  volume: number;
  pipActive: boolean;
  fullscreen: boolean;
  /** Run a control (mouse click shares this with D-pad OK). */
  onActivate: (id: ControlId) => void;
  /** Hover moves focus (§15). */
  onFocus: (id: ControlId) => void;
  onVolume: (v: number) => void;
}

/** Circular control button matching the design (state-driven focus ring). */
function Circle({
  id,
  size,
  focused,
  label,
  onActivate,
  onFocus,
  children,
}: Readonly<{
  id: ControlId;
  size: string;
  focused: boolean;
  label: string;
  onActivate: (id: ControlId) => void;
  onFocus: (id: ControlId) => void;
  children: ReactNode;
}>) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={() => onActivate(id)}
      onMouseEnter={() => onFocus(id)}
      className={`${CTRL} ${size} ${focused ? CTRL_ON_FOCUS : CTRL_OFF}`}
    >
      {children}
    </button>
  );
}

/**
 * The middle control row (§4): centered transport (rewind / play / forward) plus
 * the feature-flagged cluster on the right (next / volume / subtitles / audio /
 * settings / pip / fullscreen). The `controls` array is already filtered by the
 * feature flags, so this only renders what is present (no dead buttons). Matches
 * the 10-foot layout of the design (62 / 80 / 62 transport, 56 cluster circles).
 */
export function ControlCluster({
  controls,
  focused,
  playing,
  muted,
  volume,
  pipActive,
  fullscreen,
  onActivate,
  onFocus,
  onVolume,
}: Readonly<ControlClusterProps>) {
  const t = useT();
  const transport = controls.filter((c) => TRANSPORT.has(c));
  const cluster = controls.filter((c) => !TRANSPORT.has(c));

  const render = (id: ControlId) => {
    const on = focused === id;
    switch (id) {
      case 'rewind':
        return (
          <Circle
            key={id}
            id={id}
            size="h-[62px] w-[62px]"
            focused={on}
            label={t('player.back10')}
            onActivate={onActivate}
            onFocus={onFocus}
          >
            <IconBack10 size={27} />
          </Circle>
        );
      case 'play':
        return (
          <button
            key={id}
            type="button"
            aria-label={playing ? t('player.pause') : t('player.play')}
            onClick={() => onActivate(id)}
            onMouseEnter={() => onFocus(id)}
            className={`${CTRL} h-20 w-20 text-accent-ink ${on ? PLAY_ON_FOCUS : 'bg-accent'}`}
          >
            {playing ? <IconPause size={33} /> : <IconPlay size={35} />}
          </button>
        );
      case 'forward':
        return (
          <Circle
            key={id}
            id={id}
            size="h-[62px] w-[62px]"
            focused={on}
            label={t('player.fwd10')}
            onActivate={onActivate}
            onFocus={onFocus}
          >
            <IconFwd10 size={27} />
          </Circle>
        );
      case 'next':
        return (
          <Circle
            key={id}
            id={id}
            size="h-14 w-14"
            focused={on}
            label={t('player.nextEpisode')}
            onActivate={onActivate}
            onFocus={onFocus}
          >
            <IconNext size={24} />
          </Circle>
        );
      case 'volume':
        return (
          <VolumeControl
            key={id}
            focused={on}
            muted={muted}
            volume={volume}
            onFocus={() => onFocus(id)}
            onToggle={() => onActivate(id)}
            onVolume={onVolume}
            label={t('player.volume')}
            muteLabel={t('player.mute')}
          />
        );
      case 'subtitles':
        return (
          <Circle
            key={id}
            id={id}
            size="h-14 w-14"
            focused={on}
            label={t('player.subtitles')}
            onActivate={onActivate}
            onFocus={onFocus}
          >
            <IconSubtitles size={25} />
          </Circle>
        );
      case 'audio':
        return (
          <Circle
            key={id}
            id={id}
            size="h-14 w-14"
            focused={on}
            label={t('player.audioTrack')}
            onActivate={onActivate}
            onFocus={onFocus}
          >
            <IconAudioTrack size={24} />
          </Circle>
        );
      case 'settings':
        return (
          <Circle
            key={id}
            id={id}
            size="h-14 w-14"
            focused={on}
            label={t('player.settings')}
            onActivate={onActivate}
            onFocus={onFocus}
          >
            <IconGear size={25} />
          </Circle>
        );
      case 'pip':
        return (
          <button
            key={id}
            type="button"
            aria-label={t('player.pip')}
            onClick={() => onActivate(id)}
            onMouseEnter={() => onFocus(id)}
            className={`${CTRL} h-14 w-14 ${pipActive ? 'text-accent' : ''} ${on ? CTRL_ON_FOCUS : CTRL_OFF}`}
          >
            <IconPip size={23} />
          </button>
        );
      case 'fullscreen':
        return (
          <Circle
            key={id}
            id={id}
            size="h-14 w-14"
            focused={on}
            label={t('player.fullscreen')}
            onActivate={onActivate}
            onFocus={onFocus}
          >
            {fullscreen ? <IconFullscreenExit size={23} /> : <IconFullscreen size={23} />}
          </Circle>
        );
    }
  };

  return (
    <div className="flex items-center pt-1">
      <div className="flex-1" />
      <div className="flex items-center gap-5">{transport.map(render)}</div>
      <div className="flex flex-1 items-center justify-end gap-3.5">{cluster.map(render)}</div>
    </div>
  );
}

/** Volume as an always-expanded pill (§4b): mute button + inline slider. */
function VolumeControl({
  focused,
  muted,
  volume,
  onFocus,
  onToggle,
  onVolume,
  label,
  muteLabel,
}: Readonly<{
  focused: boolean;
  muted: boolean;
  volume: number;
  onFocus: () => void;
  onToggle: () => void;
  onVolume: (v: number) => void;
  label: string;
  muteLabel: string;
}>) {
  const trackRef = useRef<HTMLButtonElement>(null);
  const level = muted ? 0 : volume;
  // The fill/thumb track the perceptual slider position, not the raw amplitude,
  // so the handle sits under the pointer while the audio follows the loudness
  // curve (a linear fader would look wrong against a tapered volume).
  const sliderPos = muted ? 0 : volumeToSlider(volume);
  let volIcon: ReactNode;
  if (level === 0) volIcon = <IconMute size={24} />;
  else if (level < 0.5) volIcon = <IconVolLow size={24} />;
  else volIcon = <IconVolHigh size={24} />;

  const setAt = useCallback(
    (clientX: number) => {
      const el = trackRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      onVolume(sliderToVolume(clamp01((clientX - r.left) / r.width)));
    },
    [onVolume],
  );

  const onDown = useCallback(
    (e: ReactPointerEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      setAt(e.clientX);
      const move = (ev: PointerEvent) => setAt(ev.clientX);
      const up = () => {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', up);
    },
    [setAt],
  );

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: hover just moves focus (pointer parity with the D-pad); activation is via the inner buttons.
    <div
      onMouseEnter={onFocus}
      className={`flex h-14 flex-none items-center overflow-hidden rounded-full transition-[transform,box-shadow,background] duration-150 ease-out ${focused ? CTRL_ON_FOCUS : CTRL_OFF}`}
    >
      <button
        type="button"
        aria-label={muteLabel}
        onClick={onToggle}
        className="flex h-14 w-14 flex-none cursor-pointer items-center justify-center rounded-full border-none bg-transparent text-white outline-none"
      >
        {volIcon}
      </button>
      <button
        type="button"
        ref={trackRef}
        aria-label={label}
        onPointerDown={onDown}
        className="flex h-14 w-24 cursor-pointer touch-none items-center border-none bg-transparent pr-5 outline-none"
      >
        <div className="relative h-1.5 w-full rounded-full bg-[rgba(255,255,255,0.22)]">
          <div
            className="absolute inset-y-0 left-0 rounded-full bg-accent"
            style={{ width: `${sliderPos * 100}%` }}
          />
          <div
            className="absolute top-1/2 h-[13px] w-[13px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-white shadow-[0_1px_4px_rgba(0,0,0,0.5)]"
            style={{ left: `${sliderPos * 100}%` }}
          />
        </div>
      </button>
    </div>
  );
}
