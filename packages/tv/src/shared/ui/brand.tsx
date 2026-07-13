// The LUMA brand mark and the 10-foot wall clock.

import { useEffect, useState } from 'react';

/** The LUMA brand mark concentric amber rings + the wordmark. */
export function LumaMark({ size = 30 }: Readonly<{ size?: number }>) {
  return (
    <div className="flex items-center gap-3">
      <svg width={size} height={size} viewBox="0 0 32 32" fill="none" aria-hidden="true">
        <circle cx="16" cy="16" r="13" stroke="#F4B642" strokeWidth="2.4" />
        <circle cx="16" cy="16" r="4.5" fill="#F4B642" />
      </svg>
      <span
        className="font-display font-extrabold leading-none tracking-[0.16em]"
        style={{ fontSize: Math.round(size * 0.82) }}
      >
        LUMA
      </span>
    </div>
  );
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
