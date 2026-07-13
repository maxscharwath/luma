// LAN auto-discovery for the LUMA server.
//
// Browsers / TV webviews can't browse mDNS from JavaScript. Two strategies,
// tried in order:
//   1. Named candidates `http://luma.local:4040` (works where the client OS
//      resolves the mDNS `.local` hostname the server advertises: desktop,
//      mobile; NOT Samsung Tizen).
//   2. Subnet scan get this device's own LAN IP (Tizen/webOS system API, or a
//      WebRTC trick) and probe every host on its /24 for `/api/health`. This is
//      what makes discovery work on a TV with no mDNS resolution.
// The first server to answer `{ status: "ok" }` wins.

export interface DiscoverOptions {
  /** Named origins probed first. Default: `http://luma.local:4040`. */
  candidates?: string[];
  /** Per-probe timeout (ms). Default 2000. */
  timeoutMs?: number;
  /** Scan the local /24 if the named candidates miss. Default true. */
  scanSubnet?: boolean;
  /** Server port to scan for. Default 4040. */
  port?: number;
  /** Max concurrent probes during a subnet scan. Default 48. */
  concurrency?: number;
  fetch?: typeof globalThis.fetch;
}

export const DEFAULT_DISCOVERY_CANDIDATES = ['http://luma.local:4040'];

/** Probe candidates, then (optionally) the local subnet; resolve the first live
 *  LUMA server origin, or `null`. */
export async function discoverServer(opts: DiscoverOptions = {}): Promise<string | null> {
  const fetchFn = opts.fetch ?? globalThis.fetch?.bind(globalThis);
  if (!fetchFn) return null;
  const port = opts.port ?? 4040;

  // 1) Named candidates (mDNS hostname / baked default).
  const named = (opts.candidates ?? DEFAULT_DISCOVERY_CANDIDATES).map(stripTrailingSlash);
  const namedHit = await raceForServer(named, fetchFn, opts.timeoutMs ?? 2000);
  if (namedHit) return namedHit;

  // 2) Subnet scan needs this device's own LAN IP.
  if (opts.scanSubnet !== false) {
    const ip = await getLocalIPv4();
    if (ip) {
      const hosts = subnetCandidates(ip, port);
      const scanHit = await raceForServer(
        hosts,
        fetchFn,
        opts.timeoutMs ?? 1500,
        opts.concurrency ?? 48,
      );
      if (scanHit) return scanHit;
    }
  }
  return null;
}

/** All `http://<prefix>.1..254:<port>` origins for the /24 containing `ip`
 *  (excluding the device's own address). */
export function subnetCandidates(ip: string, port = 4040): string[] {
  const m = /^(\d{1,3}\.\d{1,3}\.\d{1,3})\.(\d{1,3})$/.exec(ip);
  if (!m) return [];
  const prefix = m[1];
  const self = Number(m[2]);
  const hosts: string[] = [];
  for (let i = 1; i <= 254; i++) {
    if (i !== self) hosts.push(`http://${prefix}.${i}:${port}`);
  }
  return hosts;
}

/** Best-effort local IPv4: Tizen/webOS network APIs, then a WebRTC fallback. */
export async function getLocalIPv4(): Promise<string | null> {
  return (await tizenLocalIp()) ?? (await webosLocalIp()) ?? (await webrtcLocalIp());
}

// ----- per-platform local IP --------------------------------------------------

function tizenLocalIp(): Promise<string | null> {
  const tizen = (globalThis as { tizen?: TizenSystemInfo }).tizen;
  const si = tizen?.systeminfo;
  if (!si?.getPropertyValue) return Promise.resolve(null);
  return new Promise((resolve) => {
    let settled = false;
    const finish = (v: string | null) => {
      if (!settled) {
        settled = true;
        resolve(v);
      }
    };
    const good = (ip?: string) => (ip && ip !== '0.0.0.0' ? ip : null);
    try {
      si.getPropertyValue(
        'WIFI_NETWORK',
        (w) => {
          const ip = good(w?.ipAddress);
          if (ip) return finish(ip);
          si.getPropertyValue(
            'ETHERNET_NETWORK',
            (e) => finish(good(e?.ipAddress)),
            () => finish(null),
          );
        },
        () =>
          si.getPropertyValue(
            'ETHERNET_NETWORK',
            (e) => finish(good(e?.ipAddress)),
            () => finish(null),
          ),
      );
    } catch {
      finish(null);
    }
    setTimeout(() => finish(null), 1500);
  });
}

function webosLocalIp(): Promise<string | null> {
  const svc = (globalThis as { webOS?: WebOSBridge }).webOS?.service;
  if (!svc?.request) return Promise.resolve(null);
  return new Promise((resolve) => {
    let settled = false;
    const finish = (v: string | null) => {
      if (!settled) {
        settled = true;
        resolve(v);
      }
    };
    try {
      svc.request('luna://com.palm.connectionmanager', {
        method: 'getStatus',
        parameters: {},
        onSuccess: (res) => finish(res?.wired?.ipAddress ?? res?.wifi?.ipAddress ?? null),
        onFailure: () => finish(null),
      });
    } catch {
      finish(null);
    }
    setTimeout(() => finish(null), 1500);
  });
}

function webrtcLocalIp(): Promise<string | null> {
  const RTC = (globalThis as { RTCPeerConnection?: typeof RTCPeerConnection }).RTCPeerConnection;
  if (!RTC) return Promise.resolve(null);
  return new Promise((resolve) => {
    let settled = false;
    const finish = (v: string | null) => {
      if (!settled) {
        settled = true;
        try {
          pc.close();
        } catch {
          /* ignore */
        }
        resolve(v);
      }
    };
    let pc: RTCPeerConnection;
    try {
      pc = new RTC({ iceServers: [] });
      pc.createDataChannel('luma');
      pc.onicecandidate = (e) => {
        const cand = e.candidate?.candidate;
        if (!cand) return;
        // Ignore mDNS-obfuscated candidates (`*.local`); take a private IPv4.
        const ip = /\b(\d{1,3}(?:\.\d{1,3}){3})\b/.exec(cand)?.[1];
        if (ip && isPrivateIPv4(ip)) finish(ip);
      };
      void pc.createOffer().then((o) => pc.setLocalDescription(o));
    } catch {
      return finish(null);
    }
    setTimeout(() => finish(null), 1500);
  });
}

function isPrivateIPv4(ip: string): boolean {
  return /^10\./.test(ip) || /^192\.168\./.test(ip) || /^172\.(1[6-9]|2\d|3[01])\./.test(ip);
}

// ----- probing ----------------------------------------------------------------

/** Probe `urls` (≤ `concurrency` at a time); resolve the first that is a live
 *  LUMA server, or `null` when all fail. */
function raceForServer(
  urls: string[],
  fetchFn: typeof globalThis.fetch,
  timeoutMs: number,
  concurrency = urls.length,
): Promise<string | null> {
  return new Promise((resolve) => {
    if (urls.length === 0) return resolve(null);
    let next = 0;
    let active = 0;
    let done = 0;
    let settled = false;
    const total = urls.length;

    const pump = () => {
      while (active < concurrency && next < total && !settled) {
        const url = urls[next++];
        if (url === undefined) break;
        active += 1;
        void probe(fetchFn, url, timeoutMs).then((ok) => {
          active -= 1;
          done += 1;
          if (ok && !settled) {
            settled = true;
            resolve(url);
          } else if (done === total && !settled) {
            resolve(null);
          } else {
            pump();
          }
        });
      }
    };
    pump();
  });
}

async function probe(
  fetchFn: typeof globalThis.fetch,
  base: string,
  timeoutMs: number,
): Promise<boolean> {
  try {
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), timeoutMs);
    const res = await fetchFn(`${base}/api/health`, { signal: ctrl.signal });
    clearTimeout(timer);
    if (!res.ok) return false;
    const body = (await res.json()) as { status?: string };
    return body?.status === 'ok';
  } catch {
    return false;
  }
}

function stripTrailingSlash(url: string): string {
  return url.replace(/\/+$/, '');
}

// ----- minimal platform typings -----------------------------------------------

interface TizenNetwork {
  ipAddress?: string;
}
interface TizenSystemInfo {
  systeminfo?: {
    getPropertyValue(
      prop: 'WIFI_NETWORK' | 'ETHERNET_NETWORK',
      onSuccess: (data: TizenNetwork) => void,
      onError?: () => void,
    ): void;
  };
}
interface WebOSBridge {
  service?: {
    request(
      uri: string,
      params: {
        method: string;
        parameters?: unknown;
        onSuccess?: (res: { wired?: TizenNetwork; wifi?: TizenNetwork }) => void;
        onFailure?: () => void;
      },
    ): void;
  };
}
