// Offline downloads into the app's documents directory, indexed in a small
// JSON manifest and played back by the engine straight from disk. When the
// device can direct-play the original file it is downloaded RAW (byte-identical,
// zero server work); otherwise the server's /download endpoint remuxes it on
// the fly to a fragmented MP4 the phone can decode, so EVERY title is
// downloadable on every platform.

import type { KromaClient, MediaItem } from '@kroma/core';
import type * as FileSystem from 'expo-file-system/legacy';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { Alert } from 'react-native';
import { useT } from '../i18n';
import {
  type DownloadEntry,
  type DownloadState,
  deleteEntryFiles,
  readIndex,
  sweepOrphans,
  writeIndex,
} from './store';
import { CANCELLED, runTransfer } from './transfer';

export type { DownloadEntry, DownloadState, OfflineSub } from './store';
export { formatBytes } from './store';

/** One transfer at a time: a season enqueued in bulk must not spawn a remux
 * ffmpeg per episode on the server. */
const MAX_CONCURRENT = 1;

interface DownloadsApi {
  entries: DownloadEntry[];
  /** Currently downloading titles (progress -1 = size unknown). */
  downloading: { item: MediaItem; progress: number }[];
  /** Titles waiting in the download queue (one transfer runs at a time). */
  queuedItems: MediaItem[];
  stateFor(itemId: string): DownloadState;
  /** Whether this item can be taken offline on this device at all. */
  canDownload(item: MediaItem): boolean;
  start(item: MediaItem): void;
  cancel(itemId: string): void;
  remove(itemId: string): Promise<void>;
  totalBytes: number;
}

const Ctx = createContext<DownloadsApi | null>(null);

export function useDownloads(): DownloadsApi {
  const value = useContext(Ctx);
  if (!value) throw new Error('useDownloads outside DownloadsProvider');
  return value;
}

export function DownloadsProvider({
  client,
  children,
}: Readonly<{
  client: KromaClient | null;
  children: ReactNode;
}>) {
  const t = useT();
  const [entries, setEntries] = useState<DownloadEntry[]>([]);
  const [active, setActive] = useState<Record<string, number>>({});
  const [queuedIds, setQueuedIds] = useState<string[]>([]);
  // Mirrors `entries` for the handlers, so persistence stays OUT of the state
  // updater: a reducer that also writes files runs twice under StrictMode and
  // is skipped entirely when the React Compiler memoizes the render.
  const entriesRef = useRef<DownloadEntry[]>([]);
  const runningRef = useRef<Set<string>>(new Set());
  const activeItemsRef = useRef<Map<string, MediaItem>>(new Map());
  const tasksRef = useRef<Map<string, FileSystem.DownloadResumable>>(new Map());
  const queueRef = useRef<string[]>([]);
  /** Cancels that arrived before the transfer had a handle to cancel. */
  const cancelledRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    void (async () => {
      const stored = await readIndex();
      entriesRef.current = stored;
      setEntries(stored);
      // Reclaim whatever a killed transfer left behind before anything new runs.
      await sweepOrphans(stored);
    })();
  }, []);

  /** The single write path for the index: ref, state and disk together. */
  const commitEntries = useCallback((next: DownloadEntry[]) => {
    entriesRef.current = next;
    setEntries(next);
    void writeIndex(next);
  }, []);

  const syncQueue = useCallback((next: string[]) => {
    queueRef.current = next;
    setQueuedIds(next);
  }, []);

  // The remux endpoint makes every title downloadable; keep the check for the
  // rare item with no file at all.
  const canDownload = useCallback(
    (item: MediaItem) => item.files.length > 0 || !!item.container,
    [],
  );

  const runDownload = useCallback(
    (item: MediaItem) => {
      if (!client || runningRef.current.has(item.id)) return;
      runningRef.current.add(item.id);
      setActive((a) => ({ ...a, [item.id]: 0 }));
      void (async () => {
        try {
          const entry = await runTransfer(client, item, {
            onTask: (task) => {
              tasksRef.current.set(item.id, task);
              // A cancel that landed while the task was being created has no
              // handle to act on; honour it now instead of downloading anyway.
              return !cancelledRef.current.delete(item.id);
            },
            onProgress: (frac) => setActive((a) => ({ ...a, [item.id]: frac })),
          });
          commitEntries([...entriesRef.current.filter((e) => e.itemId !== item.id), entry]);
        } catch (err) {
          const cancelled = err instanceof Error && err.message === CANCELLED;
          // Cancels are user-initiated; real failures must be VISIBLE (e.g. a
          // server without the /download endpoint, or a truncated transfer).
          if (!cancelled) {
            console.log(
              `[downloads] FAILED ${item.id}: ${err instanceof Error ? err.message : String(err)}`,
            );
            Alert.alert(item.metadata?.title ?? item.title, t('offline.failed'));
          }
        } finally {
          runningRef.current.delete(item.id);
          cancelledRef.current.delete(item.id);
          activeItemsRef.current.delete(item.id);
          tasksRef.current.delete(item.id);
          setActive((a) => {
            const { [item.id]: _dropped, ...rest } = a;
            return rest;
          });
          pumpRef.current?.();
        }
      })();
    },
    [client, commitEntries, t],
  );

  // pump() lives behind a ref so runDownload's finally can call the latest one.
  // Written in an effect, never during render: a ref mutated in the render body
  // goes stale the moment a render is memoized away or replayed.
  const pumpRef = useRef<(() => void) | null>(null);
  const pump = useCallback(() => {
    while (runningRef.current.size < MAX_CONCURRENT && queueRef.current.length > 0) {
      const [id, ...rest] = queueRef.current;
      if (!id) break;
      syncQueue(rest);
      const item = activeItemsRef.current.get(id);
      if (item) runDownload(item);
    }
  }, [runDownload, syncQueue]);
  useEffect(() => {
    pumpRef.current = pump;
  }, [pump]);

  const start = useCallback(
    (item: MediaItem) => {
      if (
        !client ||
        runningRef.current.has(item.id) ||
        queueRef.current.includes(item.id) ||
        entriesRef.current.some((e) => e.itemId === item.id)
      )
        return;
      activeItemsRef.current.set(item.id, item);
      syncQueue([...queueRef.current, item.id]);
      pump();
    },
    [client, pump, syncQueue],
  );

  const cancel = useCallback(
    (itemId: string) => {
      // Still queued: just drop it from the queue.
      if (queueRef.current.includes(itemId)) {
        syncQueue(queueRef.current.filter((id) => id !== itemId));
        activeItemsRef.current.delete(itemId);
        return;
      }
      const task = tasksRef.current.get(itemId);
      if (!task) {
        // Running, but the platform task doesn't exist yet: leave a note that
        // runDownload checks the moment it has one. Without this the tap is a
        // silent no-op and the spinner never clears.
        if (runningRef.current.has(itemId)) cancelledRef.current.add(itemId);
        return;
      }
      // cancelAsync makes the in-flight downloadAsync reject, which runs the
      // failure path (no entry registered, spinner cleared, file gone).
      void task.cancelAsync().catch(() => undefined);
    },
    [syncQueue],
  );

  const remove = useCallback(
    async (itemId: string) => {
      const entry = entriesRef.current.find((e) => e.itemId === itemId);
      commitEntries(entriesRef.current.filter((e) => e.itemId !== itemId));
      if (entry) await deleteEntryFiles(entry);
    },
    [commitEntries],
  );

  const stateFor = useCallback(
    (itemId: string): DownloadState => {
      const progress = active[itemId];
      if (progress !== undefined) return { status: 'downloading', progress };
      if (queuedIds.includes(itemId)) return { status: 'queued' };
      const entry = entries.find((e) => e.itemId === itemId);
      return entry ? { status: 'done', entry } : { status: 'none' };
    },
    [active, entries, queuedIds],
  );

  const totalBytes = useMemo(() => entries.reduce((sum, e) => sum + e.sizeBytes, 0), [entries]);

  const downloading = useMemo(
    () =>
      Object.entries(active).flatMap(([id, progress]) => {
        const item = activeItemsRef.current.get(id);
        return item ? [{ item, progress }] : [];
      }),
    [active],
  );

  const queuedItems = useMemo(
    () =>
      queuedIds.flatMap((id) => {
        const item = activeItemsRef.current.get(id);
        return item ? [item] : [];
      }),
    [queuedIds],
  );

  const value = useMemo<DownloadsApi>(
    () => ({
      entries,
      downloading,
      queuedItems,
      stateFor,
      canDownload,
      start,
      cancel,
      remove,
      totalBytes,
    }),
    [entries, downloading, queuedItems, stateFor, canDownload, start, cancel, remove, totalBytes],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
