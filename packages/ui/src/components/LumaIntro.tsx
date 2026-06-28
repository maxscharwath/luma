import { type CSSProperties, useCallback, useEffect, useRef, useState } from 'react';

/**
 * LUMA cinematic brand intro — ported from the Claude Design source
 * ("LUMA Intro.dc.html"): a total-black open, an amber glow that ignites an
 * aperture mark (ring draw → orbit glint → centre-dot ignite → shockwave), an
 * impact flash + scale punch synced to the 1.30 s bass hit, the "LUMA" wordmark
 * reveal with a metal sheen, then the tagline. Drifting embers and a vignette
 * sit on top.
 *
 * The whole timeline is choreographed to a ~5 s audio sting (bundled here, shared
 * by every client). Because browsers block autoplay-with-sound until a user
 * gesture, the visual timeline only *starts* once `audio.play()` resolves (or is
 * rejected) so picture and sound stay locked together; a pointer/key fallback
 * unblocks the sound on the first interaction, and a safety timer guarantees the
 * intro still ends even if audio never plays. There are no on-screen controls —
 * it auto-ends with the sting, and any key / remote button (OK, Back, Space) skips.
 *
 * It is intentionally framework-free — plain inline styles + an injected
 * `<style>` of @keyframes, no Tailwind — so it renders identically on the web
 * SSR shell and on old TV webviews. Mount it as a full-screen overlay and call
 * `onDone` to hand off to the app.
 *
 * `lite` (set by the TV shells) trades a little polish for a smooth frame rate on
 * weak TV GPUs: it drops the per-frame raster work desktop can absorb but a TV
 * can't — animated `filter: blur()`, the `mix-blend-mode` grain, the
 * `background-position` sheen — and keeps animation on the compositor (opacity +
 * transform only, big layers promoted with `translateZ`).
 */
export interface LumaIntroProps {
  /** Called once the intro has finished (audio ended or skipped). */
  onDone: () => void;
  /** Audio sting URL. Defaults to the bundled LUMA sting. */
  audioSrc?: string;
  /** Loop forever instead of ending (preview/idle-screen use). */
  loop?: boolean;
  /** Show the "Votre médiathèque, en grand" tagline. */
  showTagline?: boolean;
  /** Override the tagline copy. */
  tagline?: string;
  /** Performance mode for weak TV GPUs — compositor-only animation, no blur/blend
   * raster. Visually near-identical; much smoother on a TV webview. */
  lite?: boolean;
}

const DEFAULT_AUDIO = new URL('../assets/luma-intro.mp3', import.meta.url).href;

/** Fallback duration (ms) if the audio is blocked/unavailable — slightly longer
 * than the 4.992 s sting so a playing sting always reaches its own `ended`. */
const SAFETY_MS = 5400;
/** Exit fade-to-black length (ms) — matches the `transition` below. */
const EXIT_MS = 850;

const KEYFRAMES = `
@keyframes luma-igniteGlow{from{opacity:0;transform:scale(.5)}to{opacity:.5;transform:scale(1)}}
@keyframes luma-igniteGlowLite{from{opacity:0}to{opacity:.5}}
@keyframes luma-breathe{0%,100%{opacity:.38}50%{opacity:.62}}
@keyframes luma-dotIgnite{0%{opacity:0;transform:scale(0)}55%{opacity:1;transform:scale(1.55);filter:blur(2px)}75%{transform:scale(.82)}100%{opacity:1;transform:scale(1);filter:blur(0)}}
@keyframes luma-dotIgniteLite{0%{opacity:0;transform:scale(0)}60%{opacity:1;transform:scale(1.4)}80%{transform:scale(.86)}100%{opacity:1;transform:scale(1)}}
@keyframes luma-ringDraw{from{stroke-dashoffset:264}to{stroke-dashoffset:0}}
@keyframes luma-ringFade{from{opacity:0}to{opacity:1}}
@keyframes luma-orbit{from{transform:rotate(-15deg)}to{transform:rotate(360deg)}}
@keyframes luma-glintFade{0%{opacity:0}30%{opacity:1}100%{opacity:0}}
@keyframes luma-shock{0%{opacity:.75;transform:scale(.55)}100%{opacity:0;transform:scale(2.5)}}
@keyframes luma-flash{0%{opacity:0}10%{opacity:.9}100%{opacity:0}}
@keyframes luma-blackIn{0%{opacity:1}100%{opacity:0}}
@keyframes luma-punch{0%{transform:scale(.985)}38%{transform:scale(1.035)}100%{transform:scale(1)}}
@keyframes luma-wordReveal{0%{opacity:0;transform:translateY(16px) scale(.8);filter:blur(16px);text-shadow:0 0 0 rgba(242,180,66,0)}45%{opacity:1;transform:translateY(0) scale(1.06);filter:blur(0);text-shadow:0 0 30px rgba(255,214,98,.9)}68%{transform:scale(.99)}100%{opacity:1;transform:scale(1);text-shadow:0 0 14px rgba(242,180,66,.28)}}
@keyframes luma-wordRevealLite{0%{opacity:0;transform:translateY(16px) scale(.84)}55%{opacity:1;transform:translateY(0) scale(1.05)}75%{transform:scale(.99)}100%{opacity:1;transform:scale(1)}}
@keyframes luma-sheen{0%{background-position:130% 0;opacity:0}25%{opacity:1}100%{background-position:-130% 0;opacity:0}}
@keyframes luma-tagIn{0%{opacity:0;letter-spacing:.2em}100%{opacity:1;letter-spacing:.42em}}
@keyframes luma-ember{0%{opacity:0;transform:translateY(0) scale(.5)}18%{opacity:.7}100%{opacity:0;transform:translateY(-46vmin) scale(1.1)}}
@keyframes luma-flicker{0%,100%{opacity:1}48%{opacity:.86}}
@media (prefers-reduced-motion: reduce){.luma-intro *{animation-duration:.01ms !important;animation-iteration-count:1 !important;transition-duration:.01ms !important}}
`;

const GRAIN =
  "url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 width=%22120%22 height=%22120%22><filter id=%22n%22><feTurbulence type=%22fractalNoise%22 baseFrequency=%220.9%22 numOctaves=%222%22/></filter><rect width=%22100%25%22 height=%22100%25%22 filter=%22url(%23n)%22/></svg>')";

const EMBERS: ReadonlyArray<CSSProperties & { anim: string }> = [
  {
    left: '38%',
    bottom: '30%',
    width: 5,
    height: 5,
    background: '#F4B642',
    filter: 'blur(1px)',
    anim: 'luma-ember 5.5s ease-in 1.2s infinite backwards',
  },
  {
    left: '58%',
    bottom: '34%',
    width: 4,
    height: 4,
    background: '#FFD262',
    filter: 'blur(1px)',
    anim: 'luma-ember 6.2s ease-in 2.1s infinite backwards',
  },
  {
    left: '46%',
    bottom: '28%',
    width: 6,
    height: 6,
    background: '#F4B642',
    filter: 'blur(1.5px)',
    anim: 'luma-ember 6.8s ease-in 1.7s infinite backwards',
  },
  {
    left: '64%',
    bottom: '31%',
    width: 3,
    height: 3,
    background: '#FFE7A8',
    filter: 'blur(1px)',
    anim: 'luma-ember 5.9s ease-in 3s infinite backwards',
  },
  {
    left: '33%',
    bottom: '33%',
    width: 4,
    height: 4,
    background: '#F4B642',
    filter: 'blur(1px)',
    anim: 'luma-ember 7s ease-in 2.6s infinite backwards',
  },
];

const WORDMARK: CSSProperties = {
  fontFamily: "'Bricolage Grotesque', system-ui, sans-serif",
  fontWeight: 800,
  fontSize: '12vmin',
  letterSpacing: '.16em',
  whiteSpace: 'nowrap',
};

export function LumaIntro({
  onDone,
  audioSrc = DEFAULT_AUDIO,
  loop = false,
  showTagline = true,
  tagline = 'Votre médiathèque, en grand',
  lite = false,
}: Readonly<LumaIntroProps>) {
  // `started` gates the animated layers so the CSS timeline begins exactly at
  // audio onset. `runId` is the React key that restarts every animation on replay.
  const [started, setStarted] = useState(false);
  const [exiting, setExiting] = useState(false);
  const [runId, setRunId] = useState(0);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const safetyRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const exitRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const exitedRef = useRef(false);
  const loopRef = useRef(loop);
  loopRef.current = loop;
  // Latest onDone without re-running the mount effect (avoids re-arming audio).
  const onDoneRef = useRef(onDone);
  onDoneRef.current = onDone;

  const exit = useCallback(() => {
    if (exitedRef.current) return;
    exitedRef.current = true;
    clearTimeout(safetyRef.current);
    setExiting(true);
    exitRef.current = setTimeout(() => onDoneRef.current(), EXIT_MS);
  }, []);

  const start = useCallback(() => {
    exitedRef.current = false;
    clearTimeout(safetyRef.current);
    clearTimeout(exitRef.current);
    setExiting(false);
    setStarted(false);
    const a = audioRef.current;
    // Kick the visual timeline at audio onset so the flash/punch land on the
    // 1.30 s bass hit (the keyframe delays are timed to the sting).
    const begin = () => setStarted(true);
    if (a) {
      try {
        a.currentTime = 0;
      } catch {
        /* not yet seekable — harmless */
      }
      const p = a.play();
      if (p && typeof p.then === 'function') p.then(begin).catch(begin);
      else begin();
    } else {
      begin();
    }
    if (!loopRef.current) safetyRef.current = setTimeout(exit, SAFETY_MS);
  }, [exit]);

  const replay = useCallback(() => {
    clearTimeout(safetyRef.current);
    clearTimeout(exitRef.current);
    setRunId((n) => n + 1);
    start();
  }, [start]);

  useEffect(() => {
    const a = new Audio(audioSrc);
    a.preload = 'auto';
    audioRef.current = a;

    const onEnded = () => {
      if (loopRef.current) replay();
      else exit();
    };
    a.addEventListener('ended', onEnded);

    // Browsers block autoplay-with-sound until a gesture: arm a one-shot
    // unblock on the first pointer/key, then run the synced timeline.
    const unblock = () => {
      if (a.paused) {
        try {
          a.currentTime = 0;
        } catch {
          /* harmless */
        }
        void a
          .play()
          .then(() => setStarted(true))
          .catch(() => undefined);
      }
    };
    document.addEventListener('pointerdown', unblock);
    document.addEventListener('keydown', unblock);

    // Skip / replay via keyboard + TV remote (OK/Enter, Space, Back/Escape).
    const onKey = (e: KeyboardEvent) => {
      const k = e.key;
      if (
        k === 'Enter' ||
        k === ' ' ||
        k === 'Spacebar' ||
        k === 'Escape' ||
        k === 'GoBack' ||
        k === 'BrowserBack'
      ) {
        e.preventDefault();
        e.stopImmediatePropagation();
        exit();
      } else if (k === 'r' || k === 'R') {
        e.preventDefault();
        e.stopImmediatePropagation();
        replay();
      }
    };
    // Capture phase so the TV's spatial focus-nav underneath stays inert.
    window.addEventListener('keydown', onKey, true);

    start();

    return () => {
      clearTimeout(safetyRef.current);
      clearTimeout(exitRef.current);
      a.pause();
      a.removeEventListener('ended', onEnded);
      document.removeEventListener('pointerdown', unblock);
      document.removeEventListener('keydown', unblock);
      window.removeEventListener('keydown', onKey, true);
    };
    // Re-arm only if the audio source changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [audioSrc]);

  // --- Per-mode tuning: `lite` keeps everything on the compositor (opacity +
  // transform), drops the blur/blend/sheen raster, and shrinks the big layers. ---
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
    <div
      className="luma-intro"
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 9999,
        overflow: 'hidden',
        background: '#0A0A0C',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontFamily: "'Hanken Grotesk', system-ui, sans-serif",
      }}
      role="img"
      aria-label="LUMA"
    >
      <style>{KEYFRAMES}</style>

      {started ? (
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
            <div
              key={i}
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
                  boxShadow:
                    '0 0 20px 6px rgba(242,180,66,.85), 0 0 40px 12px rgba(242,180,66,.35)',
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
      ) : null}

      {/* exit transition to the app */}
      <div
        style={{
          position: 'absolute',
          inset: 0,
          background: '#0A0A0C',
          opacity: exiting ? 1 : 0,
          transition: 'opacity .8s ease',
          pointerEvents: 'none',
          zIndex: 50,
        }}
      />
    </div>
  );
}
