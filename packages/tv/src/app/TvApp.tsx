import { LumaIntro } from '@luma/ui';
import { useEffect, useState } from 'react';
import { type RedirectRule, resolveRedirect } from '#tv/app/guard';
import { AuthProvider, useAuth } from '#tv/app/providers/auth';
import { ConnectionProvider, useConnection } from '#tv/app/providers/connection';
import { ContinueProvider } from '#tv/app/providers/continue';
import { LocaleProvider } from '#tv/app/providers/locale';
import { MyListProvider } from '#tv/app/providers/mylist';
import { RecommendProvider } from '#tv/app/providers/recommend';
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
import { TvPin } from '#tv/features/accounts/TvPin';
import { TvProfileMenu } from '#tv/features/accounts/TvProfileMenu';
import { TvProfiles } from '#tv/features/accounts/TvProfiles';
import { TvQuickConnect } from '#tv/features/accounts/TvQuickConnect';
import { TvGrid } from '#tv/features/catalog/TvGrid';
import { TvHome } from '#tv/features/catalog/TvHome';
import { TvMovieDetail } from '#tv/features/catalog/TvMovieDetail';
import { TvSearch } from '#tv/features/catalog/TvSearch';
import { TvShowDetail } from '#tv/features/catalog/TvShowDetail';
import { TvPlayer } from '#tv/features/playback/TvPlayer';

export interface TvAppProps {
  /** Platform label shown in diagnostics, e.g. "Tizen" / "webOS". */
  platform?: string;
}

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
  const { connection, client, activeServerUrl, setActiveServer, setSignedIn } =
    useCatalogue(platform);
  const [introDone, setIntroDone] = useState(introAlreadySeen);

  return (
    <>
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
                <ContinueProvider>
                  <RecommendProvider>
                    <MyListProvider>
                      <TvRouterGuard />
                    </MyListProvider>
                  </RecommendProvider>
                </ContinueProvider>
              </LocaleProvider>
            </AuthProvider>
          </TvClientProvider>
        </ConnectionProvider>
      </TvNavProvider>
      {introDone ? null : (
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

/** Route → component registry. Each screen reads its own data from hooks. */
const SCREENS: TvScreens = {
  connect: TvConnect,
  profiles: TvProfiles,
  addProfile: TvAddProfile,
  quick: TvQuickConnect,
  pin: TvPin,
  profileMenu: TvProfileMenu,
  home: TvHome,
  grid: TvGrid,
  search: TvSearch,
  movie: TvMovieDetail,
  show: TvShowDetail,
  player: TvPlayer,
};

// Screen groups for the navigation guard. The profile picker is the signed-out
// home even with no servers yet — it shows just "Ajouter un profil", which opens
// the wizard (where `connect` / "Ajouter manuellement" lives). So `connect` is an
// auth-flow screen, never the launch screen.
const AUTH_SCREENS = ['profiles', 'addProfile', 'connect', 'quick', 'pin'] as const; // signed out
const APP_SCREENS = [
  'home',
  'grid',
  'search',
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
