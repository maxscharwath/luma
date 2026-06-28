// The LUMA brand intro — a full-screen, audio-synced sting shown once per browser
// session before the app. It overlays everything (including the login gate) and
// hands off to the app on completion. Client-only: Audio() and sessionStorage
// don't exist during SSR, so it renders nothing on the server and until the first
// client effect decides whether it should play (no hydration mismatch).

import { LumaIntro } from '@luma/ui';
import { useEffect, useState } from 'react';

const SEEN_KEY = 'luma:intro-seen';

export function Intro() {
  const [show, setShow] = useState(false);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);
    try {
      if (!sessionStorage.getItem(SEEN_KEY)) setShow(true);
    } catch {
      setShow(true); // private mode / storage blocked → still play it once.
    }
  }, []);

  if (!mounted || !show) return null;

  return (
    <LumaIntro
      onDone={() => {
        try {
          sessionStorage.setItem(SEEN_KEY, '1');
        } catch {
          /* ignore */
        }
        setShow(false);
      }}
    />
  );
}
