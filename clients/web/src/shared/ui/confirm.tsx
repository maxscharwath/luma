// App-wide confirm dialog as an imperative callable (react-call), replacing
// native `window.confirm` and one-off confirm modals. Call it and await the
// boolean: `if (await confirmDialog({ ... })) { ...proceed... }`. Its single
// root is mounted once at the app root (see `routes/__root.tsx`), so call sites
// carry no open-state.

import { Modal } from '@kroma/admin-kit';
import type { ReactNode } from 'react';
import { createCallable } from 'react-call';

export interface ConfirmProps {
  title: string;
  /** Optional body copy under the title. */
  message?: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  /** Render the confirm button red for a destructive action (delete/reset). */
  destructive?: boolean;
}

/** The callable. Mounted once via `<ConfirmDialog />`; opened with `.call(...)`. */
export const ConfirmDialog = createCallable<ConfirmProps, boolean>(
  ({ call, title, message, confirmLabel, cancelLabel, destructive }) => (
    <Modal title={title} onClose={() => call.end(false)}>
      {message ? <div className="mb-5 text-[13px] leading-relaxed text-dim">{message}</div> : null}
      <div className="flex justify-end gap-2.5">
        <button
          type="button"
          onClick={() => call.end(false)}
          className="rounded-md px-4 py-2.5 text-[14px] font-semibold text-muted"
        >
          {cancelLabel}
        </button>
        <button
          type="button"
          onClick={() => call.end(true)}
          className={`rounded-md px-5 py-2.5 text-[14px] font-bold ${
            destructive ? 'bg-[#E8536A] text-white' : 'bg-accent text-accent-ink'
          }`}
        >
          {confirmLabel}
        </button>
      </div>
    </Modal>
  ),
);

/** Await a yes/no confirmation. Resolves `true` when confirmed, `false` if the
 * dialog was dismissed. */
export const confirmDialog = (props: ConfirmProps): Promise<boolean> => ConfirmDialog.call(props);
