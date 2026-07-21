// Export / import dialogs for the admin "Sauvegarde" panel. Export asks whether
// to encrypt (+ password); import detects an encrypted file from its magic bytes
// and only then asks for a password, always confirming before it overwrites.

import { useT } from '@kroma/ui';
import { useState } from 'react';
import { createCallable } from 'react-call';
import { Field, Modal, ModalActions, TextInput, Toggle } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';

/** "KROMABK1\n" the encrypted-backup envelope magic (see services/backup/crypto). */
const KROMA_MAGIC = [0x4b, 0x52, 0x4f, 0x4d, 0x41, 0x42, 0x4b, 0x31, 0x0a];

/** Read a file's first bytes to tell an encrypted `.kroma` from a plain archive,
 *  so we only prompt for a password when one is actually needed. */
export async function isEncryptedFile(file: File): Promise<boolean> {
  try {
    const head = new Uint8Array(await file.slice(0, KROMA_MAGIC.length).arrayBuffer());
    return KROMA_MAGIC.every((b, i) => head[i] === b);
  } catch {
    return false;
  }
}

/** The server returns a localized message in the JSON error body surface it. */
function errMessage(e: unknown, fallback: string): string {
  const body = (e as { body?: { error?: unknown } })?.body;
  return typeof body?.error === 'string' ? body.error : fallback;
}

function ErrorLine({ text }: Readonly<{ text: string }>) {
  return <p className="mb-2 text-[13px] font-semibold text-[#E8536A]">{text}</p>;
}

/** A label + description on the left, control on the right (for toggles). */
function ToggleRow({
  label,
  hint,
  on,
  onChange,
}: Readonly<{ label: string; hint: string; on: boolean; onChange: (v: boolean) => void }>) {
  return (
    <div className="mb-4 flex items-start justify-between gap-4">
      <div>
        <div className="text-[14px] font-semibold text-text">{label}</div>
        <div className="mt-0.5 text-[12px] leading-relaxed text-dim">{hint}</div>
      </div>
      <div className="pt-0.5">
        <Toggle on={on} onChange={onChange} />
      </div>
    </div>
  );
}

export const ExportModal = createCallable<void, boolean>(({ call }) => {
  const t = useT();
  const { client } = useAuth();
  const [encrypt, setEncrypt] = useState(false);
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const canExport = !encrypt || password.trim().length > 0;

  async function run() {
    setBusy(true);
    setError(null);
    try {
      const pw = encrypt ? password.trim() : undefined;
      const blob = await client.exportBackup(pw || undefined);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `kroma-backup-${new Date().toISOString().slice(0, 10)}.kroma`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
      call.end(true);
    } catch (e) {
      setError(errMessage(e, t('admin.backupExportFailed')));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title={t('admin.backupExportTitle')} onClose={busy ? () => {} : () => call.end(false)}>
      <ToggleRow
        label={t('admin.backupEncrypt')}
        hint={t('admin.backupEncryptHint')}
        on={encrypt}
        onChange={setEncrypt}
      />
      {encrypt ? (
        <Field label={t('admin.backupPassword')} hint={t('admin.backupPasswordHint')}>
          <TextInput type="password" value={password} onChange={setPassword} className="w-full" />
        </Field>
      ) : null}
      {error ? <ErrorLine text={error} /> : null}
      <ModalActions
        onCancel={() => call.end(false)}
        cancelLabel={t('common.cancel')}
        onConfirm={() => void run()}
        confirmLabel={busy ? t('admin.backupExporting') : t('admin.backupExport')}
        busy={busy}
        disabled={!canExport}
      />
    </Modal>
  );
});

export const ImportModal = createCallable<{ file: File; encrypted: boolean }, string | null>(
  ({ call, file, encrypted }) => {
    const t = useT();
    const { client } = useAuth();
    const [password, setPassword] = useState('');
    const [reset, setReset] = useState(false);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const canImport = !encrypted || password.trim().length > 0;

    async function run() {
      setBusy(true);
      setError(null);
      try {
        const res = await client.importBackup(file, {
          password: encrypted ? password.trim() || undefined : undefined,
          reset,
        });
        call.end(t('admin.backupImported', { users: res.imported.users ?? 0 }));
      } catch (e) {
        setError(errMessage(e, t('admin.backupImportFailed')));
      } finally {
        setBusy(false);
      }
    }

    return (
      <Modal title={t('admin.backupImportTitle')} onClose={busy ? () => {} : () => call.end(null)}>
        <Field label={t('admin.backupFile')}>
          <div className="truncate rounded-[9px] border border-border-strong bg-[#0F0F13] px-3.5 py-2.25 text-[13.5px] font-semibold text-text">
            {file.name}
          </div>
        </Field>
        {encrypted ? (
          <Field label={t('admin.backupPassword')} hint={t('admin.backupEncryptedFile')}>
            <TextInput type="password" value={password} onChange={setPassword} className="w-full" />
          </Field>
        ) : null}
        <ToggleRow
          label={t('admin.backupReset')}
          hint={t('admin.backupResetHint')}
          on={reset}
          onChange={setReset}
        />
        <p className="mb-2 text-[12.5px] leading-relaxed text-dim">{t('admin.backupImportDesc')}</p>
        {error ? <ErrorLine text={error} /> : null}
        <ModalActions
          onCancel={() => call.end(null)}
          cancelLabel={t('common.cancel')}
          onConfirm={() => void run()}
          confirmLabel={busy ? t('admin.backupImporting') : t('admin.backupRestoreConfirm')}
          busy={busy}
          disabled={!canImport}
        />
      </Modal>
    );
  },
);
