// Add / edit modal for an external download client (Transmission RPC or
// qBittorrent WebUI). The embedded engine has no form: it is configured from
// the Acquisition settings page.

import { apiErrorText, type DownloadClientView, type SaveDownloadClientBody } from '@luma/core';
import { useT } from '@luma/ui';
import { useState } from 'react';
import { useAsyncAction } from '#web/features/admin/shell';
import { Field, Modal, ModalActions, SegmentedControl, TextInput } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';

type ExternalKind = 'transmission' | 'qbittorrent';

export function DownloadClientModal({
  client,
  onClose,
  onSaved,
}: Readonly<{
  /** `null` = create. */
  client: DownloadClientView | null;
  onClose: () => void;
  onSaved: () => void;
}>) {
  const t = useT();
  const { client: api } = useAuth();
  const { busy, error, run } = useAsyncAction();
  const [kind, setKind] = useState<ExternalKind>(
    client?.kind === 'qbittorrent' ? 'qbittorrent' : 'transmission',
  );
  const [name, setName] = useState(client?.name ?? '');
  const [url, setUrl] = useState(client?.url ?? '');
  const [username, setUsername] = useState(client?.username ?? '');
  const [password, setPassword] = useState('');

  const save = () =>
    run(
      async () => {
        const body: SaveDownloadClientBody = {
          kind: client ? null : kind,
          name: name.trim() || null,
          url: url.trim() || null,
          username: username.trim() || null,
          password: password || null,
          enabled: null,
          priority: null,
        };
        if (client) await api.updateDownloadClient(client.id, body);
        else await api.createDownloadClient(body);
        onSaved();
        onClose();
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  const remove = () =>
    run(
      async () => {
        if (!client) return;
        await api.deleteDownloadClient(client.id);
        onSaved();
        onClose();
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  const placeholder =
    kind === 'transmission' ? 'http://nas:9091' : 'http://nas:8080';

  return (
    <Modal title={t(client ? 'dlclients.edit' : 'dlclients.add')} onClose={onClose}>
      {!client ? (
        <Field label={t('dlclients.kind')}>
          <SegmentedControl
            value={kind}
            onChange={setKind}
            options={[
              { value: 'transmission' as const, label: 'Transmission' },
              { value: 'qbittorrent' as const, label: 'qBittorrent' },
            ]}
          />
        </Field>
      ) : null}
      <Field label={t('dlclients.name')}>
        <TextInput value={name} onChange={setName} placeholder={kind} className="w-full" />
      </Field>
      <Field label={t('dlclients.url')} hint={t('dlclients.urlHint')}>
        <TextInput value={url} onChange={setUrl} placeholder={placeholder} className="w-full" />
      </Field>
      <div className="grid grid-cols-2 gap-4">
        <Field label={t('dlclients.username')}>
          <TextInput value={username} onChange={setUsername} className="w-full min-w-0" />
        </Field>
        <Field
          label={t('dlclients.password')}
          hint={client?.hasPassword ? t('dlclients.passwordKept') : undefined}
        >
          <TextInput value={password} onChange={setPassword} type="password" className="w-full min-w-0" />
        </Field>
      </div>
      {error ? <p className="mt-1 text-[13px] font-semibold text-[#EF8091]">{error}</p> : null}
      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={save}
        confirmLabel={busy ? t('common.saving') : t('common.save')}
        busy={busy}
        disabled={!url.trim()}
        destructive={
          client && !client.builtin
            ? { label: t('dlclients.delete'), onClick: remove, disabled: busy }
            : undefined
        }
      />
    </Modal>
  );
}
