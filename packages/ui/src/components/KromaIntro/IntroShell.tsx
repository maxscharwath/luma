import type { ReactNode } from 'react';
import { KEYFRAMES } from './constants';

export interface IntroShellProps {
  /** Fade the black veil in over the intro (the hand-off to the app). */
  exiting: boolean;
  children: ReactNode;
}

/**
 * The full-screen frame both intros render into: the fixed black stage (with the
 * shared {@link KEYFRAMES}, brand font and the single "KROMA" accessible name)
 * plus the exit veil that fades to black on the way out.
 *
 * Framework-free (plain inline styles) so it renders identically on the web SSR
 * shell and on old TV webviews. The stage is flex-centred for the CSS scene;
 * the film path only holds absolutely-positioned layers, so centring is inert
 * there.
 */
export function IntroShell({ exiting, children }: Readonly<IntroShellProps>) {
  return (
    <div
      className="kroma-intro"
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
      aria-label="KROMA"
    >
      <style>{KEYFRAMES}</style>

      {children}

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
