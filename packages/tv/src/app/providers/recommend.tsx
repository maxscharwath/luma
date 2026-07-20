import type { Section, SectionItem } from '@kroma/core';
import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from 'react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';

interface Recommend {
  /** The server-assembled, ordered, localized home sections (For You, "Because
   * you watched …", themed/seasonal rows, trending, recently added). Empty until
   * `/api/home` resolves; the server already drops thin rows and localizes titles. */
  sections: Section[];
  /** Today's server-picked "En vedette" hero (multi-signal score + daily
   * rotation). Null until `/api/home/featured` resolves, on an empty catalogue
   * or against an older server the home keeps its local fallback. */
  featured: SectionItem | null;
}

const Ctx = createContext<Recommend | null>(null);

/** Home-screen recommendations for the active server. The whole home is now
 * assembled server-side (`/api/home`): ordering, localization, themed/seasonal
 * gating and de-duplication all live on the server. It's Bearer-scoped, so like
 * <ContinueProvider> it waits for a session and a reachable server (the active
 * client). Mounted inside auth + connection. */
export function RecommendProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { user } = useAuth();
  const { client } = useConnection();
  const [sections, setSections] = useState<Section[]>([]);
  const [featured, setFeatured] = useState<SectionItem | null>(null);

  useEffect(() => {
    if (!user || !client) {
      setSections([]);
      setFeatured(null);
      return;
    }
    let cancelled = false;
    client
      .home()
      .then((s) => {
        if (!cancelled) setSections(s);
      })
      .catch(() => undefined);
    client
      .featured()
      .then((f) => {
        if (!cancelled) setFeatured(f);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [user, client]);

  const value = useMemo<Recommend>(() => ({ sections, featured }), [sections, featured]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useRecommend(): Recommend {
  const c = useContext(Ctx);
  if (!c) throw new Error('useRecommend() must be used inside <RecommendProvider>');
  return c;
}
