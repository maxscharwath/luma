// Edit modal for an existing external download client (Transmission RPC or
// qBittorrent WebUI). The kind is fixed once a client exists, so this is edit-only
// (name / URL / credentials); adding a client goes through the generic
// AddEngineModal, driven by the enabled download-client engines. The embedded
// engine has no form (configured from the Acquisition settings page).

import {
  apiErrorText,
  type DownloadClientView,
  Field,
  Modal,
  ModalActions,
  type SaveDownloadClientBody,
  TextInput,
  useAdminKit,
  useAsyncAction,
  useT,
} from '@kroma/module-sdk';
import { useState } from 'react';
import { createCallable } from 'react-call';

export const DownloadClientModal = createCallable<
  { /** The external client being edited (kind is fixed). */ client: DownloadClientView },
  boolean
>(({ call, client }) => {
  const t = useT();
  const { client: api } = useAdminKit();
  const { busy, error, run } = useAsyncAction();
  const [name, setName] = useState(client.name);
  const [url, setUrl] = useState(client.url);
  const [username, setUsername] = useState(client.username);
  const [password, setPassword] = useState('');

  const save = () =>
    run(
      async () => {
        const body: SaveDownloadClientBody = {
          kind: null,
          name: name.trim() || null,
          url: url.trim() || null,
          username: username.trim() || null,
          password: password || null,
          enabled: null,
          priority: null,
        };
        await api.updateDownloadClient(client.id, body);
        call.end(true);
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  const remove = () =>
    run(
      async () => {
        await api.deleteDownloadClient(client.id);
        call.end(true);
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  return (
    <Modal title={t('dlclients.edit')} onClose={() => call.end(false)}>
      <Field label={t('dlclients.name')}>
        <TextInput value={name} onChange={setName} placeholder={client.kind} className="w-full" />
      </Field>
      <Field label={t('dlclients.url')} hint={t('dlclients.urlHint')}>
        <TextInput value={url} onChange={setUrl} className="w-full" />
      </Field>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Field label={t('dlclients.username')}>
          <TextInput value={username} onChange={setUsername} className="w-full min-w-0" />
        </Field>
        <Field
          label={t('dlclients.password')}
          hint={client.hasPassword ? t('dlclients.passwordKept') : undefined}
        >
          <TextInput
            value={password}
            onChange={setPassword}
            type="password"
            className="w-full min-w-0"
          />
        </Field>
      </div>
      {error ? <p className="mt-1 text-[13px] font-semibold text-[#EF8091]">{error}</p> : null}
      <ModalActions
        onCancel={() => call.end(false)}
        cancelLabel={t('common.cancel')}
        onConfirm={save}
        confirmLabel={busy ? t('common.saving') : t('common.save')}
        busy={busy}
        disabled={!url.trim()}
        destructive={
          client.builtin
            ? undefined
            : { label: t('dlclients.delete'), onClick: remove, disabled: busy }
        }
      />
    </Modal>
  );
});
