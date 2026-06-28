import { EMBERS, GRAIN, WORDMARK } from './constants';

export interface IntroSceneProps {
  /** React key bump — restarts every CSS animation from frame 0 on replay. */
  runId: number;
  /** Performance mode for weak TV GPUs (see {@link LumaIntroProps.lite}). */
  lite: boolean;
  showTagline: boolean;
  tagline: string;
}

/**
 * The animated visual layers of the intro (glow → aperture mark → impact flash +
 * scale punch → wordmark + sheen → tagline, plus embers / vignette / grain).
 * Pure presentation: it owns no timers or audio — the parent {@link LumaIntro}
 * mounts it only once the synced timeline has started.
 *
 * `lite` keeps everything on the compositor (opacity + transform), drops the
 * blur/blend/sheen raster, and shrinks the big layers.
 */
export function IntroScene({ runId, lite, showTagline, tagline }: Readonly<IntroSceneProps>) {
  const glowBlur = lite ? 11 : 18;
  const glowAnim = lite
    ? 'luma-igniteGlowLite 1s ease .25s both, luma-breathe 4s ease-in-out 1.4s infinite backwards'
    : 'luma-igniteGlow 1.15s ease .25s both, luma-breathe 4s ease-in-out 1.4s infinite backwards';
  const flashSize = lite ? '92vmax' : '120vmax';
  const lockupAnim = lite
    ? 'luma-punch .55s cubic-bezier(.34,1.56,.64,1) 1.27s both'
    : 'luma-punch .55s cubic-bezier(.34,1.56,.64,1) 1.27s both, luma-flicker 6s ease-in-out 2s infinite';
  const ringShadow = lite ? undefined : 'drop-shadow(0 0 7px rgba(242,180,66,.7))';
  const dotAnim = lite
    ? 'luma-dotIgniteLite .7s cubic-bezier(.22,1,.36,1) .95s both'
    : 'luma-dotIgnite .7s cubic-bezier(.22,1,.36,1) .95s both';
  const wordAnim = lite
    ? 'luma-wordRevealLite .75s cubic-bezier(.2,.9,.25,1) 1.27s both'
    : 'luma-wordReveal .75s cubic-bezier(.2,.9,.25,1) 1.27s both';
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
          animation: 'luma-blackIn .7s ease .35s both',
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
      {/* impact flash (synced to 1.30s bass hit) — opacity-only, own layer */}
      <div
        style={{
          position: 'absolute',
          width: flashSize,
          height: flashSize,
          borderRadius: '50%',
          background:
            'radial-gradient(circle, rgba(255,236,190,.9), rgba(242,180,66,.38) 16%, transparent 44%)',
          transform: 'translateZ(0)',
          animation: 'luma-flash .55s ease-out 1.27s both',
          pointerEvents: 'none',
        }}
      />

      {/* embers */}
      {embers.map(({ anim, ...s }, i) => (
        <div key={i} style={{ position: 'absolute', borderRadius: '50%', animation: anim, ...s }} />
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
        {/* aperture mark */}
        <div
          style={{
            position: 'relative',
            width: '23vmin',
            height: '23vmin',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <div
            style={{
              position: 'absolute',
              width: '62%',
              height: '62%',
              borderRadius: '50%',
              border: '2px solid rgba(242,180,66,.6)',
              animation: 'luma-shock 1.1s ease-out 1.27s both',
            }}
          />
          <svg
            viewBox="0 0 100 100"
            style={{
              position: 'absolute',
              width: '100%',
              height: '100%',
              transform: 'rotate(-90deg)',
              overflow: 'visible',
            }}
          >
            <circle
              cx="50"
              cy="50"
              r="42"
              fill="none"
              stroke="#F4B642"
              strokeWidth="3.4"
              strokeLinecap="round"
              strokeDasharray="264"
              style={{
                animation:
                  'luma-ringDraw .9s cubic-bezier(.6,0,.2,1) .4s both, luma-ringFade .3s ease .4s both',
                filter: ringShadow,
              }}
            />
          </svg>
          <div
            style={{
              position: 'absolute',
              width: '100%',
              height: '100%',
              animation:
                'luma-orbit 1s cubic-bezier(.6,0,.2,1) .4s both, luma-glintFade .7s ease 1s both',
            }}
          >
            <div
              style={{
                position: 'absolute',
                top: '4%',
                left: '50%',
                width: '7%',
                height: '7%',
                borderRadius: '50%',
                background: '#FFE7A8',
                transform: 'translateX(-50%)',
                filter: 'blur(2px)',
                boxShadow: '0 0 12px 4px rgba(255,210,98,.9)',
              }}
            />
          </div>
          <div
            style={{
              position: 'absolute',
              width: '16%',
              height: '16%',
              borderRadius: '50%',
              background: '#F4B642',
              boxShadow: '0 0 20px 6px rgba(242,180,66,.85), 0 0 40px 12px rgba(242,180,66,.35)',
              animation: dotAnim,
            }}
          />
        </div>

        {/* wordmark */}
        <div style={{ position: 'relative', lineHeight: 1 }}>
          <div style={{ ...WORDMARK, color: '#F4F3F0' }}>
            <span
              style={{
                display: 'inline-block',
                animation: wordAnim,
                textShadow: lite ? '0 0 14px rgba(242,180,66,.28)' : undefined,
                willChange: 'transform,opacity',
              }}
            >
              LUMA
            </span>
          </div>
          {/* metal sheen — repaints clipped text each frame, so high-fidelity only */}
          {lite ? null : (
            <div
              style={{
                ...WORDMARK,
                position: 'absolute',
                inset: 0,
                color: 'transparent',
                background:
                  'linear-gradient(100deg,transparent 32%,rgba(255,255,255,.92) 50%,transparent 68%)',
                WebkitBackgroundClip: 'text',
                backgroundClip: 'text',
                backgroundSize: '300% 100%',
                animation: 'luma-sheen .85s ease 2.05s both',
                pointerEvents: 'none',
              }}
            >
              LUMA
            </div>
          )}
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
              animation: 'luma-tagIn .85s ease 2.5s both',
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
