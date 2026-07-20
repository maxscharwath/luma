// The KROMA brand mark and the 10-foot wall clock.

import { Logo } from '@kroma/ui';
import { useEffect, useState } from 'react';

/** The KROMA brand lockup the wordmark with the chromatic-wheel O. `size` keeps
 * its historical meaning (rough lockup height) and maps onto the shared Logo's
 * lockup height (= wheel diameter). */
export function KromaMark({ size = 30 }: Readonly<{ size?: number }>) {
  return <Logo size={Math.round(size * 0.82)} />;
}

/** Live wall clock ("20:15") 24-hour, updated each minute. */
export function useClock(): string {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 30_000);
    return () => clearInterval(id);
  }, []);
  return now.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', hour12: false });
}
