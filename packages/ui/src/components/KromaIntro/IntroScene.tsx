import { KROMA_WHEEL_COLORS, KROMA_WHEEL_SEGMENTS } from '../KromaMark';
import { KROMA_KR_PATH, KROMA_LOCKUP, KROMA_MA_PATH } from '../kromaLockupPaths';
import { EMBERS, GRAIN } from './constants';

export interface IntroSceneProps {
  /** React key bump restarts every CSS animation from frame 0 on replay. */
  runId: number;
  /** Performance mode for weak TV GPUs (see {@link KromaIntroProps.lite}). */
  lite: boolean;
  showTagline: boolean;
  tagline: string;
}

/** Lockup height on screen (vmin); the wheel-O spans the full height. */
const LOCKUP_VMIN = 16;
/** vmin per export unit (the lockup frame is 100 units tall). */
const SCALE = LOCKUP_VMIN / KROMA_LOCKUP.height;

/**
 * The animated visual layers of the intro: the official lockup with its
 * wheel-O as the mark (glow → segments ignite while the wheel spins into
 * place → hub-glow pulse → impact flash + shockwave + scale punch → the
 * outlined KR / MA letterforms reveal → tagline, plus embers / vignette /
 * grain). Everything is SVG outlines from the brand export, no webfont.
 * Pure presentation: it owns no timers or audio the parent {@link KromaIntro}
 * mounts it only once the synced timeline has started.
 *
 * `lite` keeps everything on the compositor (opacity + transform), drops the
 * blur/blend raster, and shrinks the big layers.
 */
export function IntroScene({ runId, lite, showTagline, tagline }: Readonly<IntroSceneProps>) {
  const glowBlur = lite ? 11 : 18;
  const glowAnim = lite
    ? 'kroma-igniteGlowLite 1s ease .25s both, kroma-breathe 4s ease-in-out 1.4s infinite backwards'
    : 'kroma-igniteGlow 1.15s ease .25s both, kroma-breathe 4s ease-in-out 1.4s infinite backwards';
  const flashSize = lite ? '92vmax' : '120vmax';
  const lockupAnim = lite
    ? 'kroma-punch .55s cubic-bezier(.34,1.56,.64,1) 1.27s both'
    : 'kroma-punch .55s cubic-bezier(.34,1.56,.64,1) 1.27s both, kroma-flicker 6s ease-in-out 2s infinite';
  const hubAnim = 'kroma-hubPulse .8s cubic-bezier(.22,1,.36,1) .95s both';
  const wordAnim = lite
    ? 'kroma-wordRevealLite .75s cubic-bezier(.2,.9,.25,1) 1.27s both'
    : 'kroma-wordReveal .75s cubic-bezier(.2,.9,.25,1) 1.27s both';
  const embers = lite ? EMBERS.slice(0, 3) : EMBERS;

  return (
    // Keyed so every CSS animation restarts from frame 0 on replay.
    <div
      key={runId}
      style={{
        position: 'absolute',
        inset: 0,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      {/* opaque black opening: guarantees a total-black start */}
      <div
        style={{
          position: 'absolute',
          inset: 0,
          background: '#0A0A0C',
          zIndex: 40,
          pointerEvents: 'none',
          animation: 'kroma-blackIn .7s ease .35s both',
        }}
      />

      {/* ambient amber glow */}
      <div
        style={{
          position: 'absolute',
          width: '74vmin',
          height: '74vmin',
          borderRadius: '50%',
          background:
            'radial-gradient(circle, rgba(242,180,66,.55), rgba(242,180,66,.12) 42%, transparent 70%)',
          filter: `blur(${glowBlur}px)`,
          transform: 'translateZ(0)',
          animation: glowAnim,
        }}
      />
      {/* impact flash (synced to 1.30s bass hit) opacity-only, own layer */}
      <div
        style={{
          position: 'absolute',
          width: flashSize,
          height: flashSize,
          borderRadius: '50%',
          background:
            'radial-gradient(circle, rgba(255,236,190,.9), rgba(242,180,66,.38) 16%, transparent 44%)',
          transform: 'translateZ(0)',
          animation: 'kroma-flash .55s ease-out 1.27s both',
          pointerEvents: 'none',
        }}
      />

      {/* embers */}
      {embers.map(({ anim, ...s }) => (
        <div
          key={anim}
          style={{ position: 'absolute', borderRadius: '50%', animation: anim, ...s }}
        />
      ))}

      {/* lockup */}
      <div
        style={{
          position: 'relative',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: '5.4vmin',
          animation: lockupAnim,
          willChange: 'transform',
          backfaceVisibility: 'hidden',
        }}
      >
        {/* the official lockup: KR + the wheel-O (the animated mark) + MA.
            Segments ignite clockwise while the wheel spins into place, landing
            on the 1.27s bass hit; the letterforms reveal on the same hit. */}
        <div style={{ display: 'flex', alignItems: 'center' }}>
          <svg
            aria-hidden="true"
            width={`${KROMA_LOCKUP.krWidth * SCALE}vmin`}
            height={`${LOCKUP_VMIN}vmin`}
            viewBox={`0 0 ${KROMA_LOCKUP.krWidth} ${KROMA_LOCKUP.height}`}
            style={{ animation: wordAnim, willChange: 'transform,opacity' }}
          >
            <path d={KROMA_KR_PATH} fill="#F4F3F0" />
          </svg>

          <div
            style={{
              position: 'relative',
              width: `${LOCKUP_VMIN}vmin`,
              height: `${LOCKUP_VMIN}vmin`,
              margin: `0 ${KROMA_LOCKUP.gapRight * SCALE}vmin 0 ${KROMA_LOCKUP.gapLeft * SCALE}vmin`,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            <div
              style={{
                position: 'absolute',
                width: '70%',
                height: '70%',
                borderRadius: '50%',
                border: '2px solid rgba(242,180,66,.6)',
                animation: 'kroma-shock 1.1s ease-out 1.27s both',
              }}
            />
            {/* hub glow synced to the .95s beat (the old dot-ignite moment) */}
            <div
              style={{
                position: 'absolute',
                width: '40%',
                height: '40%',
                borderRadius: '50%',
                background:
                  'radial-gradient(circle, rgba(255,231,168,.9), rgba(242,180,66,.35) 55%, transparent 75%)',
                animation: hubAnim,
              }}
            />
            {/* spin on the <svg> itself, not an inner group: transform-box is
                missing on old TV webviews, and the wheel is centred in its viewBox */}
            <svg
              aria-hidden="true"
              viewBox="6 6 88 88"
              style={{
                position: 'absolute',
                width: '100%',
                height: '100%',
                animation:
                  'kroma-wheelSpin .87s cubic-bezier(.6,0,.2,1) .4s both, kroma-wheelIdle 9s linear 1.27s infinite',
                willChange: 'transform',
              }}
            >
              {KROMA_WHEEL_SEGMENTS.map((d, i) => (
                <path
                  key={d}
                  d={d}
                  fill={KROMA_WHEEL_COLORS[i]}
                  style={{ animation: `kroma-segIn .3s ease ${0.4 + i * 0.1}s both` }}
                />
              ))}
            </svg>
          </div>

          <svg
            aria-hidden="true"
            width={`${KROMA_LOCKUP.maWidth * SCALE}vmin`}
            height={`${LOCKUP_VMIN}vmin`}
            viewBox={`${KROMA_LOCKUP.maX} 0 ${KROMA_LOCKUP.maWidth} ${KROMA_LOCKUP.height}`}
            style={{ animation: wordAnim, willChange: 'transform,opacity' }}
          >
            <path d={KROMA_MA_PATH} fill="#F4F3F0" />
          </svg>
        </div>

        {/* tagline */}
        {showTagline ? (
          <div
            style={{
              fontWeight: 700,
              fontSize: '1.8vmin',
              letterSpacing: '.42em',
              textTransform: 'uppercase',
              color: 'rgba(244,243,240,.52)',
              whiteSpace: 'nowrap',
              animation: 'kroma-tagIn .85s ease 2.5s both',
            }}
          >
            {tagline}
          </div>
        ) : null}
      </div>

      {/* vignette (cheap radial) + grain (mix-blend raster → high-fidelity only) */}
      <div
        style={{
          position: 'absolute',
          inset: 0,
          pointerEvents: 'none',
          background: 'radial-gradient(ellipse at center, transparent 50%, rgba(0,0,0,.62))',
        }}
      />
      {lite ? null : (
        <div
          style={{
            position: 'absolute',
            inset: 0,
            pointerEvents: 'none',
            opacity: 0.05,
            mixBlendMode: 'overlay',
            background: GRAIN,
          }}
        />
      )}
    </div>
  );
}
