// Shared visual meta for request statuses: one color/label vocabulary for the
// chip on discover cards, detail pages, "Mes demandes" and the admin queue.
// Pure data (no JSX); labels resolve through i18n in the components.

import type { MessageKey, RequestStatus } from '@luma/core';

export interface RequestStatusMeta {
  labelKey: MessageKey;
  color: string;
  bg: string;
  dot: string;
  pulse?: boolean;
}

export const REQUEST_STATUS_META: Record<RequestStatus, RequestStatusMeta> = {
  pending: {
    labelKey: 'requests.st.pending',
    color: 'rgba(244,243,240,.7)',
    bg: 'rgba(255,255,255,.07)',
    dot: 'rgba(244,243,240,.45)',
  },
  approved: {
    labelKey: 'requests.st.approved',
    color: '#86A8FF',
    bg: 'rgba(134,168,255,.14)',
    dot: '#86A8FF',
  },
  searching: {
    labelKey: 'requests.st.searching',
    color: '#86A8FF',
    bg: 'rgba(134,168,255,.14)',
    dot: '#86A8FF',
    pulse: true,
  },
  downloading: {
    labelKey: 'requests.st.downloading',
    color: '#F4B642',
    bg: 'rgba(242,180,66,.15)',
    dot: '#F4B642',
    pulse: true,
  },
  importing: {
    labelKey: 'requests.st.importing',
    color: '#C792EA',
    bg: 'rgba(199,146,234,.15)',
    dot: '#C792EA',
    pulse: true,
  },
  available: {
    labelKey: 'requests.st.available',
    color: '#46D08D',
    bg: 'rgba(70,208,141,.13)',
    dot: '#46D08D',
  },
  partially_available: {
    labelKey: 'requests.st.partially_available',
    color: '#46D08D',
    bg: 'rgba(70,208,141,.09)',
    dot: 'rgba(70,208,141,.7)',
  },
  failed: {
    labelKey: 'requests.st.failed',
    color: '#E8536A',
    bg: 'rgba(232,83,106,.13)',
    dot: '#E8536A',
  },
  denied: {
    labelKey: 'requests.st.denied',
    color: '#E8536A',
    bg: 'rgba(232,83,106,.09)',
    dot: 'rgba(232,83,106,.7)',
  },
};

export const requestStatusMeta = (s: RequestStatus): RequestStatusMeta =>
  REQUEST_STATUS_META[s] ?? REQUEST_STATUS_META.pending;

/** "S1, S3" / null for a whole-show or movie request. */
export function seasonsSummary(seasons: number[] | null | undefined): string | null {
  if (!seasons || seasons.length === 0) return null;
  return seasons.map((s) => `S${s}`).join(', ');
}
