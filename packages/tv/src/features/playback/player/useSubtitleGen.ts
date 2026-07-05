import {
  GEN_LANGS,
  GEN_QUALITIES,
  type GenQuality,
  type LumaClient,
  type MediaItem,
  type SubCapabilities,
  type SubtitleGeneration,
} from '@luma/core';
import { useSubtitleGenerations } from '@luma/ui';
import { useCallback, useEffect, useMemo, useState } from 'react';
import type { SubView } from '#tv/features/playback/player/useSubtitleSelection';

export type GenMode = 'transcribe' | 'translate';

// Language + quality options live in `@luma/core` (shared with the web generate
// sheet); re-export the names the TV components already import from here.
export { GEN_LANGS, GEN_QUALITIES };

export interface GenForm {
  mode: GenMode;
  langIndex: number;
  quality: GenQuality;
  sourceIndex: number;
}

/** One focusable control in the generate sheet, driven by the remote. */
export interface GenField {
  key: 'mode' | 'lang' | 'quality' | 'source' | 'start';
  onLeft?: () => void;
  onRight?: () => void;
  onEnter?: () => void;
  /** The sheet closes after this field's OK (the "start" action). */
  closeOnEnter?: boolean;
}

export interface SubtitleGen {
  caps: SubCapabilities | null;
  /** Running / errored generations (finished ones drop into the track list). */
  pending: SubtitleGeneration[];
  form: GenForm;
  /** The focusable sheet controls for the current mode, in order. */
  fields: GenField[];
  start: () => void;
  cancel: (id: string) => void;
}

const mod = (n: number, m: number) => ((n % m) + m) % m;

/** On-device subtitle generation for the TV player: capabilities, the generate-sheet
 * form, and the remote field handlers. The in-flight poll (fire `onComplete` once
 * per finished generation, self-gate to in-flight-only) is delegated to the shared
 * `useSubtitleGenerations` hook. */
export function useSubtitleGen(
  client: LumaClient,
  item: MediaItem,
  sources: SubView[],
  onComplete: () => void,
): SubtitleGen {
  const [caps, setCaps] = useState<SubCapabilities | null>(null);
  const [form, setForm] = useState<GenForm>({
    mode: 'transcribe',
    langIndex: 0,
    quality: 'balanced',
    sourceIndex: 0,
  });
  // Shared poll: an initial fetch picks up a generation already running from an
  // earlier open (and fires `onComplete`), then self-stops once nothing is in
  // flight; `refresh` re-arms it when we kick off a new generation.
  const { generations, cancel, refresh } = useSubtitleGenerations(client, item.id, {
    active: true,
    onComplete,
  });

  useEffect(() => {
    let cancelled = false;
    client
      .subtitleCapabilities(item.id)
      .then((c) => !cancelled && setCaps(c))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, item.id]);

  const start = useCallback(() => {
    refresh(); // (re)arm polling: a new generation is about to be in flight
    const lang = GEN_LANGS[form.langIndex] ?? GEN_LANGS[0]!;
    if (form.mode === 'transcribe') {
      void client
        .generateSubtitle(item.id, {
          mode: 'transcribe',
          lang: lang.label,
          spokenLang: lang.code,
          quality: form.quality,
        })
        .catch(() => undefined);
    } else {
      const src = sources[form.sourceIndex];
      if (!src) return;
      void client
        .generateSubtitle(item.id, {
          mode: 'translate',
          lang: lang.label,
          ...(src.subId ? { sourceSubId: src.subId } : { sourceTrack: src.index }),
        })
        .catch(() => undefined);
    }
  }, [client, item.id, form, sources, refresh]);

  const setMode = useCallback((mode: GenMode) => setForm((f) => ({ ...f, mode })), []);
  const cycleLang = useCallback(
    (dir: number) =>
      setForm((f) => ({ ...f, langIndex: mod(f.langIndex + dir, GEN_LANGS.length) })),
    [],
  );
  const cycleQuality = useCallback(
    (dir: number) =>
      setForm((f) => ({
        ...f,
        quality: GEN_QUALITIES[mod(GEN_QUALITIES.indexOf(f.quality) + dir, GEN_QUALITIES.length)]!,
      })),
    [],
  );
  const cycleSource = useCallback(
    (dir: number) =>
      setForm((f) => ({
        ...f,
        sourceIndex: sources.length ? mod(f.sourceIndex + dir, sources.length) : 0,
      })),
    [sources.length],
  );

  const pending = useMemo(() => generations.filter((g) => g.status !== 'done'), [generations]);

  const fields = useMemo<GenField[]>(() => {
    const modeField: GenField = {
      key: 'mode',
      onLeft: () => setMode('transcribe'),
      onRight: () => setMode('translate'),
    };
    const langField: GenField = {
      key: 'lang',
      onLeft: () => cycleLang(-1),
      onRight: () => cycleLang(1),
    };
    const startField: GenField = { key: 'start', onEnter: start, closeOnEnter: true };
    if (form.mode === 'transcribe') {
      return [
        modeField,
        langField,
        { key: 'quality', onLeft: () => cycleQuality(-1), onRight: () => cycleQuality(1) },
        startField,
      ];
    }
    return [
      modeField,
      { key: 'source', onLeft: () => cycleSource(-1), onRight: () => cycleSource(1) },
      langField,
      startField,
    ];
  }, [form.mode, setMode, cycleLang, cycleQuality, cycleSource, start]);

  return { caps, pending, form, fields, start, cancel };
}
