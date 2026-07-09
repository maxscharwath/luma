// Add / edit modal for a Torznab indexer (Jackett / Prowlarr endpoint):
// name, torznab URL, API key (write-only secret), categories, priority.

import { apiErrorText, type IndexerView, type SaveIndexerBody } from '@luma/core';
import { useT } from '@luma/ui';
import { useState } from 'react';
import { useAsyncAction } from '#web/features/admin/shell';
import { Field, Modal, ModalActions, TextInput } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';

function parseCats(text: string): number[] {
  return text
    .split(',')
    .map((c) => c.trim())
    .filter(Boolean)
    .map(Number)
    .filter((n) => Number.isFinite(n) && n > 0);
}

export function IndexerModal({
  indexer,
  onClose,
  onSaved,
}: Readonly<{
  /** `null` = create. */
  indexer: IndexerView | null;
  onClose: () => void;
  onSaved: () => void;
}>) {
  const t = useT();
  const { client } = useAuth();
  const { busy, error, run } = useAsyncAction();
  const [name, setName] = useState(indexer?.name ?? '');
  const [url, setUrl] = useState(indexer?.url ?? '');
  const [apiKey, setApiKey] = useState('');
  const [cats, setCats] = useState((indexer?.categories ?? [2000, 5000]).join(', '));
  const [priority, setPriority] = useState(String(indexer?.priority ?? 0));

  const save = () =>
    run(
      async () => {
        const body: SaveIndexerBody = {
          name: name.trim() || null,
          url: url.trim() || null,
          // Empty = keep the stored secret on edit / no key on create.
          apiKey: apiKey.trim() || null,
          categories: parseCats(cats),
          enabled: null,
          priority: Number.parseInt(priority, 10) || 0,
        };
        if (indexer) await client.updateIndexer(indexer.id, body);
        else await client.createIndexer(body);
        onSaved();
        onClose();
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  const remove = () =>
    run(
      async () => {
        if (!indexer) return;
        await client.deleteIndexer(indexer.id);
        onSaved();
        onClose();
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  return (
    <Modal title={t(indexer ? 'indexers.edit' : 'indexers.add')} onClose={onClose}>
      <Field label={t('indexers.name')}>
        <TextInput value={name} onChange={setName} placeholder="Jackett - YGG" className="w-full" />
      </Field>
      <Field label={t('indexers.url')} hint={t('indexers.urlHint')}>
        <TextInput
          value={url}
          onChange={setUrl}
          placeholder="http://nas:9117/api/v2.0/indexers/xxx/results/torznab"
          className="w-full"
        />
      </Field>
      <Field
        label={t('indexers.apiKey')}
        hint={indexer?.hasApiKey ? t('indexers.apiKeyKept') : undefined}
      >
        <TextInput value={apiKey} onChange={setApiKey} type="password" className="w-full" />
      </Field>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Field label={t('indexers.categories')} hint={t('indexers.categoriesHint')}>
          <TextInput value={cats} onChange={setCats} className="w-full min-w-0" />
        </Field>
        <Field label={t('indexers.priority')} hint={t('indexers.priorityHint')}>
          <TextInput value={priority} onChange={setPriority} className="w-full min-w-0" />
        </Field>
      </div>
      {error ? <p className="mt-1 text-[13px] font-semibold text-[#EF8091]">{error}</p> : null}
      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={save}
        confirmLabel={busy ? t('common.saving') : t('common.save')}
        busy={busy}
        disabled={!name.trim() || !url.trim()}
        destructive={
          indexer ? { label: t('indexers.delete'), onClick: remove, disabled: busy } : undefined
        }
      />
    </Modal>
  );
}
