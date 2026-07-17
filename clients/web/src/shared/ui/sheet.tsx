import * as Dialog from '@radix-ui/react-dialog';
import type { ReactNode } from 'react';

/**
 * Routed detail modal built on Radix Dialog accessible (focus trap, Esc,
 * scroll-lock, click-outside) out of the box. Always open; closing navigates
 * back via `onClose`. Styled as the KROMA sheet.
 */
export function Sheet({
  title,
  onClose,
  children,
}: Readonly<{
  title: string;
  onClose: () => void;
  children: ReactNode;
}>) {
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/60 backdrop-blur-[18px] data-[state=open]:animate-[fade-in_.2s_ease]" />
        <Dialog.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-1/2 z-50 max-h-[88vh] w-[min(900px,calc(100%-2rem))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl border border-border bg-surface-1 shadow-pop focus:outline-none data-[state=open]:animate-[pop-in_.22s_var(--ease-spring)]"
        >
          <Dialog.Title className="sr-only">{title}</Dialog.Title>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
