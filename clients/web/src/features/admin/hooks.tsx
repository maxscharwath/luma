// Admin console data hooks (polling, busy-tracked async actions) and the
// capability/access helpers. Split out of `shell.tsx`, which re-exports these so
// call sites keep importing them from `#web/features/admin/shell`.

import { hasPermission, type Permission, type User } from '@luma/core';
import { useT } from '@luma/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useAuth } from '#web/shared/lib/auth';

/** Poll `fn` every `intervalMs` (and immediately). Re-runs when `deps` change. */
export function usePoll<T>(
  fn: () => Promise<T>,
  intervalMs: number,
  deps: unknown[],
): { data: T | null; reload: () => void } {
  const [data, setData] = useState<T | null>(null);
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const reload = useCallback(() => {
    fnRef
      .current()
      .then(setData)
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    let active = true;
    const run = () =>
      fnRef
        .current()
        .then((d) => active && setData(d))
        .catch(() => undefined);
    run();
    const iv = setInterval(run, intervalMs);
    return () => {
      active = false;
      clearInterval(iv);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return { data, reload };
}

/** A busy-tracked async action for modal save/delete handlers. `run(fn, onError?)`
 * flips `busy` while `fn` runs and, on failure, sets `error` to `onError(e)` (when
 * provided) collapsing the repeated setBusy/try/catch/finally boilerplate. */
export function useAsyncAction(): {
  busy: boolean;
  error: string | null;
  run: (fn: () => Promise<void>, onError?: (e: unknown) => string) => Promise<void>;
} {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = useCallback(async (fn: () => Promise<void>, onError?: (e: unknown) => string) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      if (onError) setError(onError(e));
    } finally {
      setBusy(false);
    }
  }, []);
  return { busy, error, run };
}

/** True if the user holds any management capability (unlocks the console).
 * `requests.manage` counts: a requests moderator needs the console shell for
 * the Demandes queue even without user/library/settings rights. */
export function isAnyAdmin(user: Pick<User, 'permissions'> | null | undefined): boolean {
  return (
    !!user &&
    (hasPermission(user, 'users.manage') ||
      hasPermission(user, 'library.manage') ||
      hasPermission(user, 'settings.manage') ||
      hasPermission(user, 'requests.manage'))
  );
}

/** Whether the current user satisfies `cap` (or is any admin when `cap` is null). */
export function useCap(cap?: Permission | null): boolean {
  const { user } = useAuth();
  if (!user) return false;
  return cap ? hasPermission(user, cap) : isAnyAdmin(user);
}

/** Full-section "access denied" panel for pages the user can't reach. */
export function Denied() {
  const t = useT();
  return (
    <div className="flex min-h-[60vh] items-center justify-center px-6">
      <div className="rounded-2xl border border-border bg-surface-1 px-8 py-10 text-center shadow-card">
        <div className="font-display text-[18px] font-bold">{t('admin.accessDenied')}</div>
        <p className="mt-2 text-[14px] text-dim">{t('admin.sectionDenied')}</p>
      </div>
    </div>
  );
}
