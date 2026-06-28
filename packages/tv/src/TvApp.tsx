import {
  type Activity,
  discoverServer,
  LumaClient,
  LumaEvents,
  type MediaItem,
  type Show,
} from '@luma/core';
import { LumaIntro } from '@luma/ui';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { AuthProvider, useAuth } from '#tv/auth';
import { type Connection, ConnectionProvider, useConnection } from '#tv/connection';
import { ContinueProvider } from '#tv/continue';
import { type RedirectRule, resolveRedirect } from '#tv/guard';
import { LocaleProvider } from '#tv/locale';
import { type DeepLink, onDeepLink, publishPreview, readDeepLink } from '#tv/preview';
import {
  type RouteName,
  TvClientProvider,
  TvNavProvider,
  TvOutlet,
  type TvScreens,
  useNav,
} from '#tv/router';
import { initialServerUrl, setServerUrl } from '#tv/server';
import { TvConnect } from '#tv/TvConnect';
import { TvHome } from '#tv/TvHome';
import { TvMovieDetail } from '#tv/TvMovieDetail';
import { TvPlayer } from '#tv/TvPlayer';
import { TvLogin, TvProfiles, TvQuickConnect, TvRegister } from '#tv/TvProfiles';
import { TvShowDetail } from '#tv/TvShowDetail';

export interface TvAppProps {
  /** Platform label shown in diagnostics, e.g. "Tizen" / "webOS". */
  platform?: string;
}

type Status = 'discovering' | 'connecting' | 'ready' | 'error';

const EMPTY_ACTIVITY: Activity = {
  phase: 'idle',
  scanning: false,
  libraries: 0,
  shows: 0,
  items: 0,
  enrichDone: 0,
  enrichTotal: 0,
  lastScanAt: null,
};
const base = (a: Activity | null): Activity => a ?? EMPTY_ACTIVITY;

// The brand intro plays once per launch. sessionStorage survives Vite HMR (so dev
// reloads don't replay it) but is fresh on a real TV cold-start.
const INTRO_SEEN_KEY = 'luma:intro-seen';
const introAlreadySeen = (() => {
  try {
    return sessionStorage.getItem(INTRO_SEEN_KEY) === '1';
  } catch {
    return false;
  }
})();

export function TvApp({ platform = 'TV' }: Readonly<TvAppProps>) {
  const [serverUrl, setUrl] = useState<string | null>(() => initialServerUrl());
  const [client, setClient] = useState<LumaClient | null>(() =>
    serverUrl ? new LumaClient({ baseUrl: serverUrl }) : null,
  );
  const [status, setStatus] = useState<Status>(serverUrl ? 'connecting' : 'discovering');
  const [movies, setMovies] = useState<MediaItem[]>([]);
  const [shows, setShows] = useState<Show[]>([]);
  const [activity, setActivity] = useState<Activity | null>(null);
  const [error, setError] = useState('');
  // A movie/show the app was launched into from a Smart Hub preview tile.
  const [deepLink, setDeepLink] = useState<DeepLink | null>(() => readDeepLink());
  // Cinematic brand intro — plays once per app launch (session), then hands off.
  const [introDone, setIntroDone] = useState(introAlreadySeen);

  const connect = useCallback((url: string, persist = true) => {
    if (persist) setServerUrl(url);
    setUrl(url);
    setClient(new LumaClient({ baseUrl: url }));
  }, []);

  const discover = useCallback(() => {
    setStatus('discovering');
    let cancelled = false;
    void discoverServer().then((found) => {
      if (cancelled) return;
      if (found) connect(found);
      else setStatus('error');
    });
    return () => {
      cancelled = true;
    };
  }, [connect]);

  // No saved/baked address → auto-discover on the LAN.
  useEffect(() => {
    if (!serverUrl) return discover();
  }, [serverUrl, discover]);

  const load = useCallback(async (c: LumaClient) => {
    setStatus('connecting');
    try {
      const [mvs, shs] = await Promise.all([c.movies(), c.shows()]);
      setMovies(mvs);
      setShows(shs);
      setStatus('ready');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setStatus('error');
    }
  }, []);

  useEffect(() => {
    if (client) void load(client);
  }, [client, load]);

  // Quiet refetch (no "connecting" flicker) for live updates.
  const refresh = useCallback(async (c: LumaClient) => {
    try {
      const [mvs, shs] = await Promise.all([c.movies(), c.shows()]);
      setMovies(mvs);
      setShows(shs);
    } catch {
      /* keep current data on a transient error */
    }
  }, []);

  // Live sync: hold the event stream open and refetch when the catalog changes
  // (scan finished, or TMDB art resolved) — no relaunch needed. A leading+trailing
  // throttle coalesces bursts (e.g. enrichment of thousands of titles) into at
  // most one refetch per window.
  useEffect(() => {
    if (!client) return;
    const MIN_MS = 2500;
    let last = 0;
    let trailing: ReturnType<typeof setTimeout> | undefined;
    const run = () => {
      last = Date.now();
      void refresh(client);
    };
    const trigger = () => {
      const since = Date.now() - last;
      if (since >= MIN_MS) {
        run();
      } else {
        clearTimeout(trailing);
        trailing = setTimeout(run, MIN_MS - since);
      }
    };
    const events = new LumaEvents(client.baseUrl, {
      // On (re)connect, grab the current scan/enrich snapshot.
      onOpen: () =>
        void client
          .status()
          .then(setActivity)
          .catch(() => undefined),
      onEvent: (e) => {
        switch (e.type) {
          case 'scan.started':
            setActivity((a) => ({ ...base(a), phase: 'scanning', scanning: true }));
            break;
          case 'scan.completed':
            setActivity((a) => ({
              ...base(a),
              phase: 'ready',
              scanning: false,
              libraries: e.libraries,
              shows: e.shows,
              items: e.items,
            }));
            trigger();
            break;
          case 'enrich.progress':
            setActivity((a) => ({
              ...base(a),
              phase: 'enriching',
              enrichDone: e.done,
              enrichTotal: e.total,
            }));
            break;
          case 'enrich.completed':
            setActivity((a) => ({
              ...base(a),
              phase: 'ready',
              enrichDone: e.resolved,
              enrichTotal: e.total,
            }));
            trigger();
            break;
          case 'library.updated':
          case 'item.updated':
          case 'show.updated':
            trigger();
            break;
          default:
            break;
        }
      },
    });
    events.connect();
    return () => {
      clearTimeout(trailing);
      events.close();
    };
  }, [client, refresh]);

  // Smart Hub preview (Samsung TV): keep the home-screen carousel (resume +
  // recently-added) in sync. Debounced so a burst of catalog updates coalesces.
  useEffect(() => {
    if (status !== 'ready' || !client) return;
    const id = setTimeout(() => void publishPreview(client, movies), 1500);
    return () => clearTimeout(id);
  }, [status, client, movies]);

  // Honour a tile selection that re-targets the app while it's already running.
  useEffect(() => onDeepLink(setDeepLink), []);

  // The router renders every screen — no view-gating `if`s. Each screen reads its
  // own data from hooks (useConnection / useAuth / useParams / useContinue), so the
  // registry holds bare components and <TvOutlet/> is prop-free.
  const connection = useMemo<Connection>(
    () => ({
      platform,
      status,
      serverUrl,
      error,
      client,
      movies,
      shows,
      activity,
      deepLink,
      connect,
      discover,
      clearDeepLink: () => setDeepLink(null),
    }),
    [
      platform,
      status,
      serverUrl,
      error,
      client,
      movies,
      shows,
      activity,
      deepLink,
      connect,
      discover,
    ],
  );

  return (
    <>
      <TvNavProvider screens={SCREENS}>
        <ConnectionProvider value={connection}>
          <TvClientProvider client={client}>
            <AuthProvider client={client}>
              <LocaleProvider client={client}>
                <ContinueProvider>
                  <TvRouterGuard />
                </ContinueProvider>
              </LocaleProvider>
            </AuthProvider>
          </TvClientProvider>
        </ConnectionProvider>
      </TvNavProvider>
      {introDone ? null : (
        // `lite` = compositor-only animation for a smooth frame rate on TV GPUs.
        // No buttons: it auto-ends with the sting, OK/Back on the remote skips.
        <LumaIntro
          lite
          onDone={() => {
            try {
              sessionStorage.setItem(INTRO_SEEN_KEY, '1');
            } catch {
              /* ignore */
            }
            setIntroDone(true);
          }}
        />
      )}
    </>
  );
}

/** Route → component registry (the "route tree"). Each screen is a bare component
 * that reads its own data from hooks — so `<TvOutlet/>` needs no props. */
const SCREENS: TvScreens = {
  connect: TvConnect,
  profiles: TvProfiles,
  login: TvLogin,
  register: TvRegister,
  quick: TvQuickConnect,
  home: TvHome,
  movie: TvMovieDetail,
  show: TvShowDetail,
  player: TvPlayer,
};

// Screen groups for the navigation guard below.
const CONNECT_SCREENS = ['connect'] as const; // pre-session discovery / connection
const AUTH_SCREENS = ['profiles', 'login', 'register', 'quick'] as const; // signed-out
const APP_SCREENS = ['home', 'movie', 'show', 'player'] as const; // signed-in app

interface GuardState {
  ready: boolean;
  signedIn: boolean;
}

// The guard only ever redirects to a param-less screen (so `nav.replace(target)`
// needs no params).
type GuardTarget = 'connect' | 'profiles' | 'home';

// Declarative navigation policy (first match wins), replacing the old nested
// `if (status) … else if (!user) …` ladder: each rule says which screens are
// allowed in a given state, and where to send the user otherwise.
const GUARD: readonly RedirectRule<GuardState, RouteName, GuardTarget>[] = [
  { when: (s) => !s.ready, to: 'connect', allow: CONNECT_SCREENS }, // not connected → connect
  { when: (s) => !s.signedIn, to: 'profiles', allow: AUTH_SCREENS }, // connected, signed out → auth flow
  { when: () => true, to: 'home', allow: APP_SCREENS }, // signed in → the app (connect/auth bounce home)
];

/** Drives the route from connection status + session and applies Smart-Hub deep
 * links, then renders the routed screen. Mounted inside every provider. */
function TvRouterGuard() {
  const nav = useNav();
  const { status, deepLink, movies, shows, clearDeepLink } = useConnection();
  const { user } = useAuth();

  // Apply the declarative guard: it returns the screen we must be on (or null to
  // stay). `replace` = single-screen stack, so there's nothing to "go back" to.
  useEffect(() => {
    const target = resolveRedirect(
      GUARD,
      { ready: status === 'ready', signedIn: Boolean(user) },
      nav.route.name,
    );
    if (target) nav.replace(target);
  }, [status, user, nav]);

  // Apply a pending Smart-Hub deep link once signed in and its target is loaded.
  useEffect(() => {
    if (!user || !deepLink) return;
    if (deepLink.type === 'movie') {
      const movie = movies.find((m) => m.id === deepLink.id);
      if (movie) {
        nav.reset('movie', { item: movie });
        clearDeepLink();
      }
    } else {
      const show = shows.find((s) => s.id === deepLink.id);
      if (show) {
        nav.reset('show', { show });
        clearDeepLink();
      }
    }
  }, [user, deepLink, movies, shows, nav, clearDeepLink]);

  return <TvOutlet />;
}
