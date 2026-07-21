import { type CompatVerdict, checkServerCompat, type KromaClient } from '@kroma/core';
import { useCallback, useEffect, useRef, useState } from 'react';
import { CLIENT_BUILD } from '#tv/app/clientBuild';
import { startHealthMonitor } from '#tv/app/healthMonitor';

/** Heartbeat cadence while the server answers normally. */
const ONLINE_MS = 8000;
/** Faster cadence while offline so a returning server is picked up quickly. */
const OFFLINE_MS = 3000;
/** A probe that hasn't answered by now counts as offline (a dead server never
 * refuses the TCP connection cleanly, so a bare fetch would hang forever). */
const TIMEOUT_MS = 4000;

/**
 * Polls the active server's `/api/health` to keep an `online` flag for the UI,
 * and calls `onReconnect` on every offline→online edge so the caller can refetch
 * whatever went stale while the server was gone. Probes slowly when healthy and
 * quickly when down (snappy auto-reconnect), and exposes `recheck()` so a dropped
 * event-stream can force an immediate probe instead of waiting for the next tick.
 *
 * Gated on `enabled` (a live session): the signed-out picker makes no requests.
 * The loop itself lives in {@link startHealthMonitor} (unit-tested separately).
 */
export function useServerHealth(
  client: KromaClient | null,
  enabled: boolean,
  onReconnect?: () => void,
): { online: boolean; recheck: () => void; serverVersion: string | null; compat: CompatVerdict } {
  const [online, setOnline] = useState(true);
  // The active server's reported version (from the same health probe) + the
  // client<->server compatibility verdict derived from it.
  const [serverVersion, setServerVersion] = useState<string | null>(null);
  // Ref so a fresh `onReconnect` closure each render doesn't restart the monitor.
  const reconnectRef = useRef(onReconnect);
  reconnectRef.current = onReconnect;
  const kickRef = useRef<() => void>(() => {});

  useEffect(() => {
    if (!client || !enabled) {
      // No active session → assume reachable so the picker shows no offline chrome.
      setOnline(true);
      setServerVersion(null); // a switched/absent server must not keep a stale version
      return;
    }
    const monitor = startHealthMonitor({
      // One health request, bounded by a short timeout; any failure ⇒ offline.
      probe: async () => {
        const ctrl = new AbortController();
        const to = setTimeout(() => ctrl.abort(), TIMEOUT_MS);
        try {
          const health = await client.health({ signal: ctrl.signal });
          setServerVersion(health.version);
          return true;
        } catch {
          return false;
        } finally {
          clearTimeout(to);
        }
      },
      onChange: setOnline,
      onReconnect: () => reconnectRef.current?.(),
      onlineMs: ONLINE_MS,
      offlineMs: OFFLINE_MS,
    });
    kickRef.current = monitor.recheck;
    return () => {
      kickRef.current = () => {};
      monitor.stop();
    };
  }, [client, enabled]);

  const recheck = useCallback(() => kickRef.current(), []);
  const compat: CompatVerdict = serverVersion
    ? checkServerCompat(CLIENT_BUILD, serverVersion)
    : 'ok';
  return { online, recheck, serverVersion, compat };
}
