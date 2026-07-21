// Shared display metadata for problem reports: per-category and per-status label
// keys + accent colours, used by the admin queue and its drawer (kept here so
// neither file imports the other).

import type { MessageKey, ReportCategory, ReportStatus, ReportSubjectKind } from '@kroma/core';

export interface Meta {
  labelKey: MessageKey;
  color: string;
}

const CATEGORY: Record<ReportCategory, Meta> = {
  metadata: { labelKey: 'report.category.metadata', color: '#86A8FF' },
  video: { labelKey: 'report.category.video', color: '#F4B642' },
  audio: { labelKey: 'report.category.audio', color: '#46D08D' },
  subtitles: { labelKey: 'report.category.subtitles', color: '#B98BF0' },
  other: { labelKey: 'report.category.other', color: '#9AA0AA' },
};

const STATUS: Record<ReportStatus, Meta> = {
  open: { labelKey: 'reports.status.open', color: '#F4B642' },
  resolved: { labelKey: 'reports.status.resolved', color: '#46D08D' },
  dismissed: { labelKey: 'reports.status.dismissed', color: '#9AA0AA' },
};

const KIND: Record<ReportSubjectKind, MessageKey> = {
  movie: 'reports.kind.movie',
  show: 'reports.kind.show',
  episode: 'reports.kind.episode',
};

export function categoryMeta(c: ReportCategory): Meta {
  return CATEGORY[c] ?? CATEGORY.other;
}

export function statusMeta(s: ReportStatus): Meta {
  return STATUS[s] ?? STATUS.open;
}

export function kindLabelKey(k: ReportSubjectKind): MessageKey {
  return KIND[k] ?? KIND.movie;
}

/** A translucent background for an accent colour (8-digit hex alpha). */
export function soft(color: string): string {
  return `${color}22`;
}
