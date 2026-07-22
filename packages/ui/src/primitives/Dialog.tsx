// <Dialog>: the modal panel (confirmations, PIN entry, track pickers).
//
// Built on React Native's <Modal>, which react-native-web implements too, so one
// component covers all four targets. On Apple TV the modal is presented as its
// own view controller and the OS focus engine is naturally confined to it; on the
// web the panel declares a focus SCOPE (`data-focus-scope`) that the spatial
// navigator honours, so the D-pad cannot wander back into the page behind.

import type { ReactNode } from 'react';
import { Modal } from 'react-native';
import { useFocusNav } from '../focus/nav';
import { Box } from '../system/Box';
import { colors } from '../tokens';
import { Button } from './Button';
import { Txt } from './Text';

export interface DialogProps {
  open: boolean;
  /** Back / Escape / a press on the backdrop. */
  onClose?: () => void;
  title?: string;
  description?: string;
  children?: ReactNode;
  /** Action row pinned to the bottom of the panel. */
  footer?: ReactNode;
  /** Panel width. Defaults to a comfortable 10-foot reading measure. */
  width?: number;
}

export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = 720,
}: Readonly<DialogProps>) {
  if (!open) return null;
  return (
    <Modal transparent visible animationType="fade" onRequestClose={onClose}>
      <DialogSurface onClose={onClose} width={width} title={title} description={description}>
        {children}
        {footer}
      </DialogSurface>
    </Modal>
  );
}

/** Split out so `useFocusNav` mounts WITH the panel: that arms the press guard
 * and moves focus into the dialog exactly as a screen transition would. */
function DialogSurface({
  onClose,
  width,
  title,
  description,
  children,
}: Readonly<Omit<DialogProps, 'open'> & { width: number }>) {
  useFocusNav({ onBack: onClose });
  return (
    <Box flex center bg={colors.overlay} p={64}>
      <Box
        w={width}
        maxW="100%"
        bg="surface2"
        radius="2xl"
        border="borderStrong"
        shadow="pop"
        p={40}
        gap={24}
        dataSet={FOCUS_SCOPE}
      >
        {title ? <Txt variant="h2">{title}</Txt> : null}
        {description ? (
          <Txt color="textMuted" variant="body">
            {description}
          </Txt>
        ) : null}
        {children}
      </Box>
    </Box>
  );
}

/** Marks the subtree the web spatial navigator must stay inside. Ignored by the
 * native targets, whose OS focus engine already confines focus to the modal. */
const FOCUS_SCOPE = { focusScope: '' } as const;

/** The conventional action row: secondary on the left, primary on the right. */
export function DialogFooter({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <Box row justify="flex-end" gap={12} mt={8}>
      {children}
    </Box>
  );
}

export interface ConfirmDialogProps extends Omit<DialogProps, 'footer' | 'children'> {
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: () => void;
  /** Paint the confirm action as destructive. */
  destructive?: boolean;
}

/** The common case: a question with a cancel and a confirm. */
export function ConfirmDialog({
  confirmLabel,
  cancelLabel,
  onConfirm,
  destructive = false,
  ...dialog
}: Readonly<ConfirmDialogProps>) {
  return (
    <Dialog
      {...dialog}
      footer={
        <DialogFooter>
          <Button variant="ghost" label={cancelLabel} onPress={dialog.onClose} />
          <Button
            variant={destructive ? 'danger' : 'primary'}
            label={confirmLabel}
            onPress={onConfirm}
            autoFocus
          />
        </DialogFooter>
      }
    />
  );
}
