// The shared radial backdrop for the TV auth / connect / pin screens.

import type { ReactNode } from 'react';

/** The shared centred backdrop for the auth / connect / pin screens. Scrolling
 * lives on the outer element and the content centres in an inner `min-h-full`
 * wrapper so it sits centred when it fits but scrolls from the top (never
 * clipping the title) when the content is taller than the screen. */
export function AuthScreen({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <div
      className="scrollbar-none fixed inset-0 z-10 overflow-y-auto animate-[tv-fade-in_0.45s_ease]"
      style={{ background: 'radial-gradient(120% 90% at 50% 0%, #15131C, #0A0A0C 68%)' }}
    >
      <div className="flex min-h-full flex-col items-center justify-center px-10 py-12 text-center">
        {children}
      </div>
    </div>
  );
}
