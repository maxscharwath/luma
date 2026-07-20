import { KromaMark, type KromaMarkSpin } from './KromaMark';
import { KROMA_KR_PATH, KROMA_LOCKUP, KROMA_MA_PATH } from './kromaLockupPaths';

export interface LogoProps {
  /** Lockup height in px (= the wheel-O diameter); with `markOnly`, the wheel diameter. */
  size?: number;
  /** Show only the chromatic wheel, without the KR MA letters. */
  markOnly?: boolean;
  /** Rotate the wheel: "idle" (ambient 9s) or "loading" (spinner 2.6s). */
  spin?: KromaMarkSpin;
}

/**
 * KROMA brand lockup, drawn entirely from the official export's outlines
 * (kromaLockupPaths): "KR" + the chromatic wheel as the O + "MA". No webfont
 * involved, so it renders identically even offline. The wheel is its own
 * `<svg>` element (not a nested group) so `spin` stays legacy-TV-safe, and its
 * hub is a true hole so the lockup works on any surface. Letters inherit
 * `currentColor`, themed via --kroma-text.
 */
export function Logo({ size = 24, markOnly = false, spin }: Readonly<LogoProps>) {
  if (markOnly) return <KromaMark size={size} spin={spin} />;
  const s = size / KROMA_LOCKUP.height;
  return (
    <span
      role="img"
      aria-label="KROMA"
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        whiteSpace: 'nowrap',
        color: 'var(--kroma-text, #F4F3F0)',
      }}
    >
      <svg
        width={KROMA_LOCKUP.krWidth * s}
        height={size}
        viewBox={`0 0 ${KROMA_LOCKUP.krWidth} ${KROMA_LOCKUP.height}`}
        aria-hidden="true"
      >
        <path d={KROMA_KR_PATH} fill="currentColor" />
      </svg>
      <KromaMark
        size={size}
        spin={spin}
        style={{
          margin: `0 ${KROMA_LOCKUP.gapRight * s}px 0 ${KROMA_LOCKUP.gapLeft * s}px`,
        }}
      />
      <svg
        width={KROMA_LOCKUP.maWidth * s}
        height={size}
        viewBox={`${KROMA_LOCKUP.maX} 0 ${KROMA_LOCKUP.maWidth} ${KROMA_LOCKUP.height}`}
        aria-hidden="true"
      >
        <path d={KROMA_MA_PATH} fill="currentColor" />
      </svg>
    </span>
  );
}
