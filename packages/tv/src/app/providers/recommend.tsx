import type { KromaClient, Section, SectionItem } from '@kroma/core';
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';
import { getExo } from '#tv/features/playback/player/engine';

/** Flatten the home sections (recently added + suggestions) into the KROMA
 * preview-channel program list the native Android shell consumes. Movies only:
 * the Watch Next / preview deep link resolves a movie id. The art is the public
 * composited card (backdrop + KROMA logo), same vignette as Tizen. */
function toHomeChannel(sections: Section[], client: KromaClient) {
  const seen = new Set<string>();
  const out: { id: string; title: string; subtitle: string; imageUrl: string; kind: string }[] = [];
  for (const s of sections) {
    for (const e of s.items) {
      if (e.type !== 'movie' || seen.has(e.item.id)) continue;
      seen.add(e.item.id);
      const m = e.item;
      out.push({
        id: m.id,
        title: m.title,
        subtitle: m.year ? String(m.year) : '',
        imageUrl: `${client.baseUrl}/api/items/${encodeURIComponent(m.id)}/card?v=${encodeURIComponent(m.addedAt)}`,
        kind: 'movie',
      });
      if (out.length >= 40) return out; // preview channels cap ~50; stay lean
    }
  }
  return out;
}

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

  // Mirror the recently-added + suggested titles into a KROMA preview channel on
  // the Android TV / Google TV launcher home. Guarded on the serialized payload
  // (the effect re-runs on render churn) so it pushes once per real change.
  // No-op off the Android shell (getExo() null / no method).
  const lastPushed = useRef<string>('');
  useEffect(() => {
    const exo = getExo();
    if (!exo?.setHomeChannel || !client) return;
    const json = JSON.stringify(toHomeChannel(sections, client));
    if (json === lastPushed.current) return;
    // Don't create an empty channel on the first (pre-load) render; an empty push
    // is only meaningful as a clear AFTER we've published something.
    if (json === '[]' && lastPushed.current === '') return;
    lastPushed.current = json;
    exo.setHomeChannel(json);
  }, [sections, client]);

  const value = useMemo<Recommend>(() => ({ sections, featured }), [sections, featured]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useRecommend(): Recommend {
  const c = useContext(Ctx);
  if (!c) throw new Error('useRecommend() must be used inside <RecommendProvider>');
  return c;
}
