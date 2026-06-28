import type { CSSProperties } from 'react';

// Static assets / timing / keyframes for the LUMA cinematic intro. Kept apart
// from the component + scene so the choreography numbers live in one place.

export const DEFAULT_AUDIO = new URL('../../assets/luma-intro.mp3', import.meta.url).href;

/** Fallback duration (ms) if the audio is blocked/unavailable — slightly longer
 * than the 4.992 s sting so a playing sting always reaches its own `ended`. */
export const SAFETY_MS = 5400;
/** Exit fade-to-black length (ms) — matches the `transition` below. */
export const EXIT_MS = 850;

export const KEYFRAMES = `
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

export const GRAIN =
  "url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 width=%22120%22 height=%22120%22><filter id=%22n%22><feTurbulence type=%22fractalNoise%22 baseFrequency=%220.9%22 numOctaves=%222%22/></filter><rect width=%22100%25%22 height=%22100%25%22 filter=%22url(%23n)%22/></svg>')";

export const EMBERS: ReadonlyArray<CSSProperties & { anim: string }> = [
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

export const WORDMARK: CSSProperties = {
  fontFamily: "'Bricolage Grotesque', system-ui, sans-serif",
  fontWeight: 800,
  fontSize: '12vmin',
  letterSpacing: '.16em',
  whiteSpace: 'nowrap',
};
