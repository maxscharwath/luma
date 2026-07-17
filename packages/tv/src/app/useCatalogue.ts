import {
  type Activity,
  discoverServer,
  forgetServer as forgetServerStore,
  KromaClient,
  KromaEvents,
  loadSession,
  type MediaItem,
  normalizeServerUrl as norm,
  type SavedServer,
  type Show,
  saveServer as saveServerStore,
} from '@kroma/core';
import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Connection } from '#tv/app/providers/connection';
import { useServerHealth } from '#tv/app/useServerHealth';
import { type DeepLink, onDeepLink, publishPreview, readDeepLink } from '#tv/shared/preview';
import { initialServers } from '#tv/shared/server';

type Status = 'discovering' | 'connecting' | 'ready' | 'error';

/** A readable name for a server URL (saved label, else the host). */
function serverLabel(servers: SavedServer[], url: string | null): string | null {
  if (!url) return null;
  const saved = servers.find((s) => s.url === norm(url));
  if (saved?.name) return saved.name;
  try {
    return new URL(url).hostname;
  } catch {
    return url;
  }
}

const EMPTY_ACTIVITY: Activity = {
  phase: 'idle',
  scanning: false,
  libraries: 0,
  shows: 0,
  items: 0,
  enrichDone: 0,
  enrichTotal: 0,
  probeDone: 0,
  probeTotal: 0,
  lastScanAt: null,
};
const base = (a: Activity | null): Activity => a ?? EMPTY_ACTIVITY;

/** What the catalogue hook exposes to the shell: the connection context value
 * plus the few handles the auth provider needs wired directly. */
export interface Catalogue {
  connection: Connection;
  client: KromaClient | null;
  activeServerUrl: string | null;
  setActiveServer: (url: string) => void;
  setSignedIn: (v: boolean) => void;
}

/**
 * Owns the TV's multi-server connection + catalogue state: discovery, the active
 * client, the movies/shows catalogue, the live event stream, Smart-Hub preview
 * publishing and deep links. Returns the `Connection` context value plus the
 * handles the auth provider needs (client / active server / signed-in toggle).
 */
export function useCatalogue(platform: string): Catalogue {
  // The session present at boot used to point the first client at the right
  // server with its token already applied (no flicker on "Reprendre").
  const bootSession = useMemo(() => loadSession(), []);
  const [servers, setServers] = useState<SavedServer[]>(() => initialServers());
  const [activeServerUrl, setActiveUrl] = useState<string | null>(
    () => bootSession?.serverUrl ?? servers[0]?.url ?? null,
  );

  const client = useMemo<KromaClient | null>(() => {
    if (!activeServerUrl) return null;
    // No initial bearer: the auth provider exchanges the active account's access
    // token for a session token and calls `setAuthToken` (+ installs the refresh
    // handler) once the session belongs to this server.
    return new KromaClient({ baseUrl: activeServerUrl });
  }, [activeServerUrl]);

  // Reported up by the auth provider; gates the catalogue + event stream so the
  // signed-out picker makes no requests at all.
  const [signedIn, setSignedIn] = useState(Boolean(bootSession));
  const [status, setStatus] = useState<Status>(activeServerUrl ? 'connecting' : 'discovering');
  const [movies, setMovies] = useState<MediaItem[]>([]);
  const [shows, setShows] = useState<Show[]>([]);
  const [activity, setActivity] = useState<Activity | null>(null);
  const [error, setError] = useState('');
  const [discovering, setDiscovering] = useState(false);
  const [discovered, setDiscovered] = useState<string[]>([]);
  const [deepLink, setDeepLink] = useState<DeepLink | null>(() => readDeepLink());

  const setActiveServer = useCallback((url: string) => setActiveUrl(norm(url)), []);

  const addServer = useCallback((url: string, name?: string | null) => {
    const next = saveServerStore({ url, name });
    setServers(next);
    setActiveUrl(norm(url));
  }, []);

  const forgetServer = useCallback(
    (url: string) => {
      const u = norm(url);
      // Drop it from core storage (also clears its accounts + active session).
      forgetServerStore(u);
      const next = servers.filter((s) => s.url !== u);
      setServers(next);
      if (activeServerUrl && norm(activeServerUrl) === u) setActiveUrl(next[0]?.url ?? null);
    },
    [servers, activeServerUrl],
  );

  const discover = useCallback(() => {
    setDiscovering(true);
    let cancelled = false;
    void discoverServer().then((found) => {
      if (cancelled) return;
      setDiscovering(false);
      if (found) {
        setDiscovered((d) => (d.includes(found) ? d : [...d, found]));
        // First-run bootstrap: no servers yet → adopt the discovered one.
        if (servers.length === 0) addServer(found);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [servers.length, addServer]);

  // No saved servers → auto-discover on the LAN (first run).
  useEffect(() => {
    if (servers.length === 0) return discover();
    setStatus((s) => (s === 'discovering' ? 'connecting' : s));
  }, [servers.length, discover]);

  // Fetch the catalogue. `quiet` skips the status/error toggles (used by the live
  // refetch below no "connecting" flicker, keep current data on a transient error).
  const fetchCatalogue = useCallback(async (c: KromaClient, quiet = false) => {
    if (!quiet) setStatus('connecting');
    try {
      const [mvs, shs] = await Promise.all([c.movies(), c.shows()]);
      setMovies(mvs);
      setShows(shs);
      if (!quiet) setStatus('ready');
    } catch (err) {
      if (!quiet) {
        setError(err instanceof Error ? err.message : String(err));
        setStatus('error');
      }
    }
  }, []);

  // Load the catalogue only once a profile is active the signed-out picker
  // stays silent (no /api/movies, /api/shows before sign-in).
  useEffect(() => {
    if (client && signedIn) void fetchCatalogue(client);
  }, [client, signedIn, fetchCatalogue]);

  // Heartbeat: detect when the server drops and auto-refetch when it returns.
  const { online, recheck } = useServerHealth(client, signedIn, () => {
    if (client) void fetchCatalogue(client, true);
  });

  // Live sync: hold the event stream open and refetch when the catalog changes.
  // A leading+trailing throttle coalesces bursts into at most one refetch/window.
  // Only while signed in the picker keeps the stream (and /api/status) closed.
  useEffect(() => {
    if (!client || !signedIn) return;
    const MIN_MS = 2500;
    let last = 0;
    let trailing: ReturnType<typeof setTimeout> | undefined;
    const run = () => {
      last = Date.now();
      void fetchCatalogue(client, true);
    };
    const trigger = () => {
      const since = Date.now() - last;
      if (since >= MIN_MS) run();
      else {
        clearTimeout(trailing);
        trailing = setTimeout(run, MIN_MS - since);
      }
    };
    const events = new KromaEvents(client.baseUrl, {
      // The stream open/close is the fastest signal that the server just came
      // back or dropped; nudge the heartbeat to confirm reachability at once
      // rather than waiting for its next tick.
      onClose: () => recheck(),
      onOpen: () => {
        recheck();
        void client
          .status()
          .then(setActivity)
          .catch(() => undefined);
      },
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
  }, [client, signedIn, fetchCatalogue, recheck]);

  // Smart Hub preview (Samsung TV): keep the home-screen carousel in sync.
  useEffect(() => {
    if (status !== 'ready' || !client) return;
    const id = setTimeout(() => void publishPreview(client, movies), 1500);
    return () => clearTimeout(id);
  }, [status, client, movies]);

  useEffect(() => onDeepLink(setDeepLink), []);

  const connection = useMemo<Connection>(
    () => ({
      platform,
      status,
      servers,
      activeServerUrl,
      activeServerName: serverLabel(servers, activeServerUrl),
      error,
      online,
      client,
      movies,
      shows,
      activity,
      discovering,
      discovered,
      deepLink,
      addServer,
      setActiveServer,
      discover,
      forgetServer,
      clearDeepLink: () => setDeepLink(null),
    }),
    [
      platform,
      status,
      servers,
      activeServerUrl,
      error,
      online,
      client,
      movies,
      shows,
      activity,
      discovering,
      discovered,
      deepLink,
      addServer,
      setActiveServer,
      discover,
      forgetServer,
    ],
  );

  return { connection, client, activeServerUrl, setActiveServer, setSignedIn };
}
