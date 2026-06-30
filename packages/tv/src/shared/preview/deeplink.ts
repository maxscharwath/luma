// Decoding the tile selection that launched or re-targeted the app.

import { tizen } from '#tv/shared/preview/tizen';
import type { DeepLink } from '#tv/shared/preview/types';

function asDeepLink(obj: unknown): DeepLink | null {
  if (obj && typeof obj === 'object') {
    const o = obj as Record<string, unknown>;
    if ((o.type === 'movie' || o.type === 'show') && typeof o.id === 'string') {
      return { type: o.type, id: o.id };
    }
  }
  return null;
}

/** Decode a tile's PAYLOAD. The platform delivers our `action_data` either
 *  verbatim, or wrapped as `{"values": "<uri-encoded JSON>"}` (the envelope
 *  Samsung's own sample unwraps) handle both. */
function parsePayload(raw: string): DeepLink | null {
  try {
    const first = JSON.parse(raw) as unknown;
    const direct = asDeepLink(first);
    if (direct) return direct;
    const values = (first as { values?: unknown })?.values;
    if (typeof values === 'string') {
      return asDeepLink(JSON.parse(decodeURIComponent(values)));
    }
  } catch {
    /* ignore malformed payloads */
  }
  return null;
}

/** The tile selection that launched/targeted the app, or null. */
export function readDeepLink(): DeepLink | null {
  const t = tizen();
  if (!t) return null;
  try {
    const req = t.application.getCurrentApplication().getRequestedAppControl();
    const payload = req?.appControl.data.find((d) => d.key === 'PAYLOAD')?.value?.[0];
    return payload ? parsePayload(payload) : null;
  } catch {
    return null;
  }
}

/** Fire `cb` when the running app is re-targeted by a preview tile. The cold
 *  launch is covered by readDeepLink(); this handles selection while open.
 *  Returns a cleanup function. */
export function onDeepLink(cb: (link: DeepLink) => void): () => void {
  if (!tizen()) return () => undefined;
  const handler = () => {
    const link = readDeepLink();
    if (link) cb(link);
  };
  window.addEventListener('appcontrol', handler);
  return () => window.removeEventListener('appcontrol', handler);
}
