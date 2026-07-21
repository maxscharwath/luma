import { KromaIntro } from '@kroma/ui';
import { lazy, useEffect, useState } from 'react';
import { CompatBanner } from '#tv/app/CompatBanner';
import { type RedirectRule, resolveRedirect } from '#tv/app/guard';
import { AuthProvider, useAuth } from '#tv/app/providers/auth';
import { ConnectionProvider, useConnection } from '#tv/app/providers/connection';
import { ContinueProvider } from '#tv/app/providers/continue';
import { EnvProvider, type TvEnvOverrides } from '#tv/app/providers/env';
import { LocaleProvider } from '#tv/app/providers/locale';
import { MyListProvider } from '#tv/app/providers/mylist';
import { RecommendProvider } from '#tv/app/providers/recommend';
import { WatchedProvider } from '#tv/app/providers/watched';
import {
  type RouteName,
  TvClientProvider,
  TvNavProvider,
  TvOutlet,
  type TvScreens,
  useNav,
} from '#tv/app/router';
import { useCatalogue } from '#tv/app/useCatalogue';
import { TvAddProfile } from '#tv/features/accounts/TvAddProfile';
import { TvConnect } from '#tv/features/accounts/TvConnect';
import { TvDeviceSettings } from '#tv/features/accounts/TvDeviceSettings';
import { TvPin } from '#tv/features/accounts/TvPin';
import { TvProfileMenu } from '#tv/features/accounts/TvProfileMenu';
import { TvProfiles } from '#tv/features/accounts/TvProfiles';
import { TvQuickConnect } from '#tv/features/accounts/TvQuickConnect';
import { TvGenreGrid } from '#tv/features/catalog/TvGenreGrid';
import { TvGenres } from '#tv/features/catalog/TvGenres';
import { TvGrid } from '#tv/features/catalog/TvGrid';
import { TvHome } from '#tv/features/catalog/TvHome';
import { TvMovieDetail } from '#tv/features/catalog/TvMovieDetail';
import { TvPerson } from '#tv/features/catalog/TvPerson';
import { TvSearch } from '#tv/features/catalog/TvSearch';
import { TvShowDetail } from '#tv/features/catalog/TvShowDetail';

// The player (its 4 playback engines + the seek / subtitle / stats stack) is the
// app's heaviest screen and is only reached once the user starts playback lazy
// it so the browse-first initial bundle stays lean. TvPlayer is a NAMED export,
// so shim it to a default for React.lazy. The `<Suspense>` that catches this
// lives in <TvOutlet> (app/router.tsx). Legacy-tier IIFE builds inline dynamic
// imports back into their single classic file, so only the modern tiers split.
const TvPlayer = lazy(() =>
  import('#tv/features/playback/TvPlayer').then((m) => ({ default: m.TvPlayer })),
);

export interface TvAppProps {
  /** Platform label shown in diagnostics, e.g. "Tizen" / "webOS". */
  platform?: string;
  /** Override input-capability detection (pointer / physical keyboard) when the
   * platform label alone is wrong e.g. a Steam Deck is 'Desktop' but gamepad-driven. */
  capabilities?: TvEnvOverrides;
  /** Shell-bundled override for the brand-intro film. TVs keep the default 4K60
   * HEVC film (hardware plane, panel upscale); the Tauri desktop shell passes a
   * 1080p grade because its transparent window (the native mpv plane sits
   * behind the webview) costs <video> the compositor fast path, so 4K frames
   * are decoded and downscaled the slow way. */
  introVideoSrc?: string;
}

// The brand intro plays once per launch. sessionStorage survives Vite HMR (so dev
// reloads don't replay it) but is fresh on a real TV cold-start.
const INTRO_SEEN_KEY = 'kroma:intro-seen';
const introAlreadySeen = (() => {
  try {
    return sessionStorage.getItem(INTRO_SEEN_KEY) === '1';
  } catch {
    return false;
  }
})();

export function TvApp({ platform = 'TV', capabilities, introVideoSrc }: Readonly<TvAppProps>) {
  const { connection, client, activeServerUrl, setActiveServer, setSignedIn } =
    useCatalogue(platform);
  const [introDone, setIntroDone] = useState(introAlreadySeen);

  return (
    <EnvProvider platform={platform} overrides={capabilities}>
      <TvNavProvider screens={SCREENS}>
        <ConnectionProvider value={connection}>
          <TvClientProvider client={client}>
            <AuthProvider
              client={client}
              activeServerUrl={activeServerUrl}
              setActiveServer={setActiveServer}
              onSignedInChange={setSignedIn}
            >
              <LocaleProvider client={client}>
                <CompatBanner />
                <ContinueProvider>
                  <RecommendProvider>
                    <MyListProvider>
                      <WatchedProvider>
                        <TvRouterGuard />
                      </WatchedProvider>
                    </MyListProvider>
                  </RecommendProvider>
                </ContinueProvider>
              </LocaleProvider>
            </AuthProvider>
          </TvClientProvider>
        </ConnectionProvider>
      </TvNavProvider>
      {introDone ? null : (
        <KromaIntro
          lite
          videoSrc={introVideoSrc}
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
    </EnvProvider>
  );
}

/** Route → component registry. Each screen reads its own data from hooks. */
const SCREENS: TvScreens = {
  connect: TvConnect,
  profiles: TvProfiles,
  addProfile: TvAddProfile,
  quick: TvQuickConnect,
  deviceSettings: TvDeviceSettings,
  pin: TvPin,
  profileMenu: TvProfileMenu,
  home: TvHome,
  grid: TvGrid,
  genres: TvGenres,
  genre: TvGenreGrid,
  search: TvSearch,
  person: TvPerson,
  movie: TvMovieDetail,
  show: TvShowDetail,
  player: TvPlayer,
};

// Screen groups for the navigation guard. The profile picker is the signed-out
// home even with no servers yet it shows just "Ajouter un profil", which opens
// the wizard (where `connect` / "Ajouter manuellement" lives). So `connect` is an
// auth-flow screen, never the launch screen.
const AUTH_SCREENS = [
  'profiles',
  'addProfile',
  'connect',
  'quick',
  'pin',
  'deviceSettings',
] as const; // signed out
const APP_SCREENS = [
  'home',
  'grid',
  'genres',
  'genre',
  'search',
  'person',
  'movie',
  'show',
  'player',
  'profileMenu',
  'pin',
] as const; // signed in (pin: set/clear)

interface GuardState {
  signedIn: boolean;
}

type GuardTarget = 'profiles' | 'home';

// Declarative navigation policy (first match wins): signed-out → the picker /
// auth flow; signed-in → the app.
const GUARD: readonly RedirectRule<GuardState, RouteName, GuardTarget>[] = [
  { when: (s) => !s.signedIn, to: 'profiles', allow: AUTH_SCREENS },
  { when: () => true, to: 'home', allow: APP_SCREENS },
];

/** Drives the route from connection + session, then renders the routed screen. */
function TvRouterGuard() {
  const nav = useNav();
  const { deepLink, movies, shows, clearDeepLink } = useConnection();
  const { user } = useAuth();

  useEffect(() => {
    const target = resolveRedirect(GUARD, { signedIn: Boolean(user) }, nav.route.name);
    if (target) nav.replace(target);
  }, [user, nav]);

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
