import type { Activity, LumaClient, MediaItem, Show } from '@luma/core';
import { createContext, type ReactNode, useContext } from 'react';
import type { DeepLink } from '#tv/preview';

export type ConnectStatus = 'discovering' | 'connecting' | 'ready' | 'error';

/** Everything the connect screen + catalogue need from the server connection.
 * Exposed via context so `TvConnect` / `TvHome` read it from a hook (no props) and
 * can be registered as bare components in the router's screen map. */
export interface Connection {
  platform: string;
  status: ConnectStatus;
  serverUrl: string | null;
  error: string;
  /** Null until a server is reached. */
  client: LumaClient | null;
  movies: MediaItem[];
  shows: Show[];
  activity: Activity | null;
  /** Pending Smart-Hub deep link, if any. */
  deepLink: DeepLink | null;
  connect: (url: string) => void;
  discover: () => void;
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
