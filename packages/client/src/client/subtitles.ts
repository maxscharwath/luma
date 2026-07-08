// On-device subtitle generation: transcribe the audio (Whisper) or translate an
// existing track (LLM), poll live progress, cancel, and delete generated tracks.

import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** A generated + cached subtitle, with its WebVTT URL (relative to the server). */
export interface DownloadedSub {
  id: string;
  language: string | null;
  label: string;
  /** `"whisper"` | `"translate"` (drives the "IA" badge). */
  provider: string;
  /** WebVTT URL, e.g. `/api/items/:id/subtitles/dl/:id.vtt`. */
  url: string;
}

/** Which generation actions the server build + config enable (to hide empty UI). */
export interface SubCapabilities {
  /** On-device Whisper transcription is compiled in. */
  transcribe: boolean;
  /** A default LLM provider is configured (for translation). */
  translate: boolean;
}

export function subtitleCapabilities(ctx: RequestContext, id: string): Promise<SubCapabilities> {
  return ctx.json<SubCapabilities>(`/items/${encodeURIComponent(id)}/subtitles/capabilities`);
}

/** Generation mode: speech-to-text, or translate an existing track. */
export type GenMode = 'transcribe' | 'translate';
/** Whisper model tier (Rapide / Équilibré / Précis). */
export type GenQuality = 'fast' | 'balanced' | 'accurate';

/** Target / spoken languages offered by the generate sheet. `code` is the Whisper
 * hint; `label` is the track name (chosen so the server resolves it back to a
 * language code). Single source of truth shared by every client's generate UI. */
export const GEN_LANGS: { code: string; label: string }[] = [
  { code: 'fr', label: 'Français' },
  { code: 'en', label: 'English' },
  { code: 'es', label: 'Español' },
  { code: 'de', label: 'Deutsch' },
  { code: 'it', label: 'Italiano' },
  { code: 'pt', label: 'Português' },
  { code: 'nl', label: 'Nederlands' },
  { code: 'ja', label: 'Japonais' },
  { code: 'ko', label: 'Coréen' },
  { code: 'zh', label: 'Chinois' },
  { code: 'ru', label: 'Russe' },
  { code: 'ar', label: 'Arabe' },
];

/** Whisper model tiers offered by the generate sheet, in order. */
export const GEN_QUALITIES: GenQuality[] = ['fast', 'balanced', 'accurate'];

/** A generation request. For `translate`, give `sourceTrack` (embedded index) or
 * `sourceSubId` (a generated track); the server resolves the source text itself. */
export interface GenerateReq {
  mode: GenMode;
  /** Target language label, e.g. "Français". */
  lang: string;
  /** Transcribe: spoken language to force (name or code); omit to auto-detect. */
  spokenLang?: string;
  /** Transcribe: model tier (default `balanced`). */
  quality?: GenQuality;
  /** Transcribe: audio-relative track index (default 0). */
  audioTrack?: number;
  /** Translate: embedded subtitle track index to translate from. */
  sourceTrack?: number;
  /** Translate: a generated subtitle id to translate from. */
  sourceSubId?: string;
}

/** A live (or recently finished) generation, as polled. `progress` is 0..1. */
export interface SubtitleGeneration {
  id: string;
  mode: GenMode;
  lang: string | null;
  /** `queued` | `model` | `extract` | `transcribe` | `translate` | `done` | `error`. */
  stage: string;
  status: 'running' | 'done' | 'error';
  progress: number;
  etaSec: number | null;
  error: string | null;
  /** The resulting downloaded-subtitle id, once `status === 'done'`. */
  subId: string | null;
}

/** Start a generation. Returns immediately with a `genId`; poll
 * {@link subtitleGenerations} for progress, then refresh the downloaded list. */
export function generateSubtitle(ctx: RequestContext, id: string, req: GenerateReq): Promise<{ genId: string }> {
  return ctx.json<{ genId: string }>(`/items/${encodeURIComponent(id)}/subtitles/generate`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(req),
  });
}

/** Live + recently-finished generations for this item. */
export function subtitleGenerations(ctx: RequestContext, id: string): Promise<SubtitleGeneration[]> {
  return ctx.json<SubtitleGeneration[]>(`/items/${encodeURIComponent(id)}/subtitles/generations`);
}

/** Request cancellation of a running generation. */
export function cancelGeneration(ctx: RequestContext, id: string, genId: string): Promise<void> {
  return ctx.json<void>(
    `/items/${encodeURIComponent(id)}/subtitles/generations/${encodeURIComponent(genId)}`,
    { method: 'DELETE' },
  );
}

/** This item's already-generated subtitles. */
export function downloadedSubtitles(ctx: RequestContext, id: string): Promise<DownloadedSub[]> {
  return ctx.json<DownloadedSub[]>(`/items/${encodeURIComponent(id)}/subtitles/downloaded`);
}

/** Delete a generated subtitle track (DB row + cached file). */
export function deleteSubtitle(ctx: RequestContext, id: string, dlId: string): Promise<void> {
  return ctx.json<void>(
    `/items/${encodeURIComponent(id)}/subtitles/downloaded/${encodeURIComponent(dlId)}`,
    { method: 'DELETE' },
  );
}
