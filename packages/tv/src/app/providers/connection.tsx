import type {
  Activity,
  CompatVerdict,
  KromaClient,
  MediaItem,
  SavedServer,
  Show,
} from '@kroma/core';
import { createContext, type ReactNode, useContext } from 'react';
import type { DeepLink } from '#tv/shared/preview';

export type ConnectStatus = 'discovering' | 'connecting' | 'ready' | 'error';

/** Multi-server connection state for the TV. The catalogue (movies/shows) and
 * `client` always reflect the **active** server; the picker / add-profile wizard
 * read `servers` and the LAN `discovered` list. Exposed via context so every
 * screen reads it from a hook (no prop-drilling) and stays a bare router entry. */
export interface Connection {
  platform: string;
  status: ConnectStatus;
  /** Every saved server, most-recently-used first. */
  servers: SavedServer[];
  /** The server the catalogue + `client` point at, or null before any is added. */
  activeServerUrl: string | null;
  /** Friendly name of the active server (host fallback). */
  activeServerName: string | null;
  error: string;
  /** Whether the active server is currently reachable. Driven by a `/api/health`
   * heartbeat (plus the event stream open/close); flips the nav-bar indicator and
   * gates the auto-refetch on reconnect. Always `true` before a server is active. */
  online: boolean;
  /** The active server's reported version (from `/api/health`), or null before a
   * server has answered. */
  serverVersion: string | null;
  /** Client<->server version compatibility (`ok` unless the server is older than
   * this client build needs); drives the non-blocking update banner. */
  compat: CompatVerdict;
  /** Client for the active server (null before any server is reached). */
  client: KromaClient | null;
  movies: MediaItem[];
  shows: Show[];
  activity: Activity | null;
  /** LAN auto-discovery, for the first-run empty state + the wizard's local list. */
  discovering: boolean;
  discovered: string[];
  /** Pending Smart-Hub deep link, if any. */
  deepLink: DeepLink | null;
  /** Add (upsert) a server and make it active. */
  addServer: (url: string, name?: string | null) => void;
  /** Switch the active server (rebuilds the client; clears the active session). */
  setActiveServer: (url: string) => void;
  /** Run LAN discovery again. */
  discover: () => void;
  /** Forget a server and every remembered account on it. */
  forgetServer: (url: string) => void;
  clearDeepLink: () => void;
}

const ConnectionCtx = createContext<Connection | null>(null);

export function ConnectionProvider({
  value,
  children,
}: Readonly<{
  value: Connection;
  children: ReactNode;
}>) {
  return <ConnectionCtx.Provider value={value}>{children}</ConnectionCtx.Provider>;
}

export function useConnection(): Connection {
  const c = useContext(ConnectionCtx);
  if (!c) throw new Error('useConnection() must be used inside <ConnectionProvider>');
  return c;
}
