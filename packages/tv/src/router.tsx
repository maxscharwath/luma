import type { LumaClient, MediaItem, PublicUser, Show } from '@luma/core';
import {
  type ComponentType,
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useState,
} from 'react';

/**
 * A tiny, type-safe, zero-dependency router for the 10-foot app — TanStack-grade
 * DX without the bundle. A TV has no address bar, so this is a *memory* history:
 * an in-memory stack of screens. Add a screen by adding one line to `TvRoutes`;
 * `go`, `reset` and `<TvOutlet>` all become type-checked against it.
 *
 *   const nav = useNav();
 *   nav.go('movie', { item });   // push (params are type-checked per route)
 *   nav.back();                  // pop one screen (Back key / "Retour")
 *   nav.reset('show', { show }); // replace the stack with home → screen (deep links)
 *
 *   <TvOutlet screens={{
 *     home:   () => <TvHome … />,
 *     movie:  ({ item }) => <TvMovieDetail item={item} … />,
 *     show:   ({ show }) => <TvShowDetail show={show} … />,
 *     player: ({ item }) => <TvPlayer item={item} … />,
 *   }} />
 */
export interface TvRoutes {
  /** Server discovery / connection screen (no client yet). */
  connect: undefined;
  /** Profile picker (signed out). */
  profiles: undefined;
  /** Password entry for a chosen profile (signed out). */
  login: { user: PublicUser };
  /** New-account creation (signed out). */
  register: undefined;
  /** Quick Connect code / QR (signed out). */
  quick: undefined;
  home: undefined;
  movie: { item: MediaItem };
  show: { show: Show };
  player: { item: MediaItem };
}

export type RouteName = keyof TvRoutes;
export type TvRoute = { [K in RouteName]: { name: K; params: TvRoutes[K] } }[RouteName];

// Call signature: routes with no params omit the second arg — `go('home')` vs `go('movie', { item })`.
type GoArgs<K extends RouteName> = TvRoutes[K] extends undefined
  ? [name: K]
  : [name: K, params: TvRoutes[K]];

export interface TvNav {
  /** The screen on top of the stack. */
  route: TvRoute;
  /** Stack depth (1 = home only). */
  depth: number;
  /** Can we go back? (depth > 1) */
  canGoBack: boolean;
  /** Push a screen. `go('movie', { item })`. */
  go: <K extends RouteName>(...args: GoArgs<K>) => void;
  /** Pop one screen (no-op at the root). */
  back: () => void;
  /** Replace the whole stack with home → screen (deep-link entry point). */
  reset: <K extends RouteName>(...args: GoArgs<K>) => void;
  /** Replace the whole stack with a single screen (no history). Used by guards
   * (connect / profiles / home) so there's nothing to "go back" to. */
  replace: <K extends RouteName>(...args: GoArgs<K>) => void;
  /** Jump straight back to the root. */
  home: () => void;
}

const CONNECT = { name: 'connect', params: undefined } as TvRoute;
const HOME = { name: 'home', params: undefined } as TvRoute;
const NavCtx = createContext<TvNav | null>(null);

/**
 * The route → component registry (the "route tree"), declared once and handed to
 * <TvNavProvider screens={…}>. Each screen reads its own params/data from hooks
 * (useParams / useClient / useAuth / useConnection), so the components take NO
 * props and `<TvOutlet/>` renders them by name — TanStack-style.
 */
export type TvScreens = { [K in RouteName]: ComponentType };
const ScreensCtx = createContext<TvScreens | null>(null);

function make<K extends RouteName>(name: K, params?: TvRoutes[K]): TvRoute {
  return { name, params } as TvRoute;
}

export function TvNavProvider({
  screens,
  children,
}: Readonly<{ screens: TvScreens; children: ReactNode }>) {
  // Start on `connect` — the app boots into discovery/connection before anything
  // else; the guard advances to profiles/home as the session resolves.
  const [stack, setStack] = useState<TvRoute[]>([CONNECT]);

  const go = useCallback(<K extends RouteName>(...[name, params]: GoArgs<K>) => {
    setStack((s) => [...s, make(name, params)]);
  }, []);
  const back = useCallback(() => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s)), []);
  const reset = useCallback(<K extends RouteName>(...[name, params]: GoArgs<K>) => {
    setStack(name === 'home' ? [HOME] : [HOME, make(name, params)]);
  }, []);
  const replace = useCallback(<K extends RouteName>(...[name, params]: GoArgs<K>) => {
    setStack([make(name, params)]);
  }, []);
  const home = useCallback(() => setStack([HOME]), []);

  const value = useMemo<TvNav>(
    () => ({
      route: stack[stack.length - 1]!,
      depth: stack.length,
      canGoBack: stack.length > 1,
      go,
      back,
      reset,
      replace,
      home,
    }),
    [stack, go, back, reset, replace, home],
  );
  return (
    <NavCtx.Provider value={value}>
      <ScreensCtx.Provider value={screens}>{children}</ScreensCtx.Provider>
    </NavCtx.Provider>
  );
}

export function useNav(): TvNav {
  const ctx = useContext(NavCtx);
  if (!ctx) throw new Error('useNav() must be used inside <TvNavProvider>');
  return ctx;
}

/** Typed access to the current route's params — `const { item } = useParams('movie')`. */
export function useParams<K extends RouteName>(name: K): TvRoutes[K] {
  const { route } = useNav();
  if (route.name !== name) throw new Error(`useParams('${name}') called on route '${route.name}'`);
  return route.params as TvRoutes[K];
}

// --- Client context: the LumaClient every screen needs, provided once at the top. ---
const ClientCtx = createContext<LumaClient | null>(null);

// Tolerates a null client (during connect, before a server is reached) so the
// providers can wrap the whole app — the `connect` screen never calls useClient().
export function TvClientProvider({
  client,
  children,
}: Readonly<{
  client: LumaClient | null;
  children: ReactNode;
}>) {
  return <ClientCtx.Provider value={client}>{children}</ClientCtx.Provider>;
}

/** The LumaClient. Throws if read before a server is reached — only the routed
 * screens (rendered once status is `ready`) call it, never the connect screen. */
export function useClient(): LumaClient {
  const c = useContext(ClientCtx);
  if (!c) throw new Error('useClient() called before the server was reached');
  return c;
}

/** Renders the component registered for the route on top of the stack. Prop-free:
 * the screen reads its own params/data from hooks. `<TvOutlet />`. */
export function TvOutlet() {
  const { route } = useNav();
  const screens = useContext(ScreensCtx);
  if (!screens) throw new Error('<TvOutlet> must be inside <TvNavProvider screens={…}>');
  const Screen = screens[route.name];
  return <Screen />;
}
