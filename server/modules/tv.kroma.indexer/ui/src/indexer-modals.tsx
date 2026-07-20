// Add / edit modals for indexers. Two kinds coexist:
//  - Torznab (Jackett / Prowlarr endpoint): name + URL + API key.
//  - Built-in (native Cardigann definition): a browse/pick step then a form
//    generated from the definition's own settings schema.

import {
  apiErrorText,
  Field,
  type IndexerDefinitionDetailView,
  type IndexerDefinitionView,
  type IndexerView,
  Modal,
  ModalActions,
  type SaveIndexerBody,
  TextInput,
  Toggle,
  OptionSelect as UiSelect,
  useAdminKit,
  useAsyncAction,
  useT,
} from '@kroma/module-sdk';
import { IconLoader2, IconSearch } from '@tabler/icons-react';
import { useEffect, useMemo, useState } from 'react';

/** Parse a comma-separated Newznab category list into positive category ids. */
export function parseCats(text: string): number[] {
  return text
    .split(',')
    .map((c) => c.trim())
    .filter(Boolean)
    .map(Number)
    .filter((n) => Number.isFinite(n) && n > 0);
}

/** Router for EDITING an existing indexer: a built-in row edits in the settings
 * form, a Torznab row in the endpoint form. Creation goes through the generic
 * add-picker (Torznab) or the definition picker (built-in), not this modal. */
export function IndexerModal({
  indexer,
  onClose,
  onSaved,
}: Readonly<{
  indexer: IndexerView;
  onClose: () => void;
  onSaved: () => void;
}>) {
  if (indexer.kind === 'builtin' && indexer.definitionId) {
    return (
      <BuiltinIndexerModal
        definitionId={indexer.definitionId}
        indexer={indexer}
        onClose={onClose}
        onSaved={onSaved}
      />
    );
  }
  return <TorznabIndexerModal indexer={indexer} onClose={onClose} onSaved={onSaved} />;
}

// ----- Torznab endpoint form ------------------------------------------------------

function TorznabIndexerModal({
  indexer,
  onClose,
  onSaved,
}: Readonly<{
  indexer: IndexerView;
  onClose: () => void;
  onSaved: () => void;
}>) {
  const t = useT();
  const { client } = useAdminKit();
  const { busy, error, run } = useAsyncAction();
  const [name, setName] = useState(indexer.name);
  const [url, setUrl] = useState(indexer.url);
  const [apiKey, setApiKey] = useState('');
  const [cats, setCats] = useState(indexer.categories.join(', '));
  const [priority, setPriority] = useState(String(indexer.priority));

  const save = () =>
    run(
      async () => {
        const body: SaveIndexerBody = {
          name: name.trim() || null,
          url: url.trim() || null,
          apiKey: apiKey.trim() || null,
          categories: parseCats(cats),
          enabled: null,
          priority: Number.parseInt(priority, 10) || 0,
        };
        await client.updateIndexer(indexer.id, body);
        onSaved();
        onClose();
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  const remove = () =>
    run(
      async () => {
        await client.deleteIndexer(indexer.id);
        onSaved();
        onClose();
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  return (
    <Modal title={t('indexers.edit')} onClose={onClose}>
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
        hint={indexer.hasApiKey ? t('indexers.apiKeyKept') : undefined}
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
        destructive={{ label: t('indexers.delete'), onClick: remove, disabled: busy }}
      />
    </Modal>
  );
}

// ----- built-in: definition picker ------------------------------------------------

/** Browse the Cardigann catalog, sync it from upstream, and pick a definition
 * to add. */
export function DefinitionPickerModal({
  onPick,
  onClose,
}: Readonly<{ onPick: (definitionId: string) => void; onClose: () => void }>) {
  const t = useT();
  const { client } = useAdminKit();
  const [defs, setDefs] = useState<IndexerDefinitionView[] | null>(null);
  const [synced, setSynced] = useState(true);
  const [q, setQ] = useState('');
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    client
      .adminIndexerDefinitions()
      .then((v) => {
        setDefs(v.definitions);
        setSynced(v.synced);
      })
      .catch((e) => setError(apiErrorText(e, t('indexers.testFailed'))));
  };
  // biome-ignore lint/correctness/useExhaustiveDependencies: load once on open
  useEffect(load, []);

  const sync = () => {
    setSyncing(true);
    setError(null);
    client
      .syncIndexerDefinitions()
      .then(() => load())
      .catch((e) => setError(apiErrorText(e, t('indexers.syncFailed'))))
      .finally(() => setSyncing(false));
  };

  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const list = defs ?? [];
    if (!needle) return list.slice(0, 200);
    return list
      .filter(
        (d) =>
          d.name.toLowerCase().includes(needle) || d.description.toLowerCase().includes(needle),
      )
      .slice(0, 200);
  }, [defs, q]);

  return (
    <Modal title={t('indexers.pickTitle')} onClose={onClose}>
      <div className="mb-3 flex items-center gap-2">
        <div className="flex min-w-0 flex-1 items-center gap-2 rounded-[9px] border border-border-strong bg-[#0F0F13] px-3">
          <IconSearch size={15} className="text-dim" />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder={t('indexers.searchDefs')}
            className="min-w-0 flex-1 bg-transparent py-2.25 text-[13.5px] font-semibold text-text outline-none"
          />
        </div>
        <button
          type="button"
          onClick={sync}
          disabled={syncing}
          className="inline-flex items-center gap-1.5 rounded-lg border border-white/12 bg-[#1A1A20] px-3 py-2 text-[12.5px] font-semibold text-white/80 hover:bg-[#222229] disabled:opacity-60"
        >
          {syncing ? <IconLoader2 size={13} stroke={2.4} className="animate-spin" /> : null}
          {t('indexers.syncDefs')}
        </button>
      </div>

      {error ? <p className="mb-2 text-[13px] font-semibold text-[#EF8091]">{error}</p> : null}

      {defs && !synced && defs.length === 0 ? (
        <p className="py-8 text-center text-[13px] text-dim">{t('indexers.syncFirst')}</p>
      ) : null}

      <div className="max-h-[46vh] overflow-y-auto">
        {(defs === null ? [] : filtered).map((d) => (
          <button
            key={d.id}
            type="button"
            onClick={() => onPick(d.id)}
            className="flex w-full items-center justify-between gap-3 border-b border-white/5 px-1 py-2.5 text-left hover:bg-white/3"
          >
            <div className="min-w-0">
              <div className="truncate text-[13.5px] font-bold text-text">{d.name}</div>
              <div className="truncate text-[12px] text-dim">{d.description || d.id}</div>
            </div>
            <span className="shrink-0 rounded-full border border-white/12 px-2 py-0.5 text-[11px] font-semibold text-white/55">
              {d.kind === 'public' ? t('indexers.public') : t('indexers.private')}
            </span>
          </button>
        ))}
        {defs === null ? (
          <p className="py-8 text-center text-[13px] text-dim">{t('indexers.loading')}</p>
        ) : null}
      </div>

      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={onClose}
        confirmLabel={t('common.close')}
      />
    </Modal>
  );
}

// ----- built-in: settings form ----------------------------------------------------

export function BuiltinIndexerModal({
  definitionId,
  indexer,
  onClose,
  onSaved,
}: Readonly<{
  definitionId: string;
  indexer: IndexerView | null;
  onClose: () => void;
  onSaved: () => void;
}>) {
  const t = useT();
  const { client } = useAdminKit();
  const { busy, error, run } = useAsyncAction();
  const [detail, setDetail] = useState<IndexerDefinitionDetailView | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [baseUrl, setBaseUrl] = useState(indexer?.url ?? '');
  const [cats, setCats] = useState((indexer?.categories ?? [2000, 5000]).join(', '));
  const [priority, setPriority] = useState(String(indexer?.priority ?? 0));

  // biome-ignore lint/correctness/useExhaustiveDependencies: load once per definition id
  useEffect(() => {
    client
      .indexerDefinitionDetail(definitionId)
      .then((d) => {
        setDetail(d);
        // Seed the form from the definition defaults (secrets stay blank; on
        // edit the server keeps stored secrets when a field is left empty).
        const seed: Record<string, string> = {};
        for (const s of d.settings) {
          if (s.kind.startsWith('info')) continue;
          seed[s.name] = s.default ?? (s.kind === 'checkbox' ? 'false' : '');
        }
        setSettings(seed);
        if (!indexer) setBaseUrl(d.links[0] ?? '');
      })
      .catch((e) => setLoadError(apiErrorText(e, t('indexers.testFailed'))));
  }, [definitionId]);

  const setField = (name: string, value: string) => setSettings((s) => ({ ...s, [name]: value }));

  const save = () =>
    run(
      async () => {
        const body: SaveIndexerBody = {
          name: detail?.name ?? null,
          url: baseUrl.trim() || null,
          apiKey: null,
          categories: parseCats(cats),
          enabled: null,
          priority: Number.parseInt(priority, 10) || 0,
          kind: 'builtin',
          definitionId,
          settings,
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

  const title = detail?.name ?? definitionId;

  return (
    <Modal title={title} onClose={onClose}>
      {loadError ? (
        <p className="mb-2 text-[13px] font-semibold text-[#EF8091]">{loadError}</p>
      ) : null}
      {detail === null && !loadError ? (
        <p className="py-8 text-center text-[13px] text-dim">{t('indexers.loading')}</p>
      ) : null}

      {detail ? (
        <div className="max-h-[52vh] overflow-y-auto pr-0.5">
          {detail.links.length > 1 ? (
            <Field label={t('indexers.baseUrl')}>
              <UiSelect
                value={baseUrl}
                onChange={setBaseUrl}
                options={detail.links.map((l) => ({ value: l, label: l }))}
              />
            </Field>
          ) : (
            <Field label={t('indexers.baseUrl')}>
              <TextInput value={baseUrl} onChange={setBaseUrl} className="w-full" />
            </Field>
          )}

          {detail.settings
            .filter((s) => !s.kind.startsWith('info'))
            .map((s) => {
              const configured = indexer?.configuredSettings.includes(s.name);
              if (s.kind === 'checkbox') {
                return (
                  <div key={s.name} className="mb-4 flex items-center justify-between gap-4">
                    <span className="text-[13.5px] font-semibold text-text">{s.label}</span>
                    <Toggle
                      on={settings[s.name] === 'true'}
                      onChange={(v) => setField(s.name, v ? 'true' : 'false')}
                    />
                  </div>
                );
              }
              if (s.kind === 'select') {
                return (
                  <Field key={s.name} label={s.label}>
                    <UiSelect
                      value={settings[s.name] ?? ''}
                      onChange={(v) => setField(s.name, v)}
                      options={s.options.map(([value, label]) => ({ value, label }))}
                    />
                  </Field>
                );
              }
              const isSecret = s.kind === 'password';
              return (
                <Field
                  key={s.name}
                  label={s.label}
                  hint={isSecret && configured ? t('indexers.apiKeyKept') : undefined}
                >
                  <TextInput
                    value={settings[s.name] ?? ''}
                    onChange={(v) => setField(s.name, v)}
                    type={isSecret ? 'password' : 'text'}
                    className="w-full"
                  />
                </Field>
              );
            })}

          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <Field label={t('indexers.categories')} hint={t('indexers.categoriesHint')}>
              <TextInput value={cats} onChange={setCats} className="w-full min-w-0" />
            </Field>
            <Field label={t('indexers.priority')} hint={t('indexers.priorityHint')}>
              <TextInput value={priority} onChange={setPriority} className="w-full min-w-0" />
            </Field>
          </div>
        </div>
      ) : null}

      {error ? <p className="mt-1 text-[13px] font-semibold text-[#EF8091]">{error}</p> : null}
      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={save}
        confirmLabel={busy ? t('common.saving') : t('common.save')}
        busy={busy}
        disabled={!detail || !baseUrl.trim()}
        destructive={
          indexer ? { label: t('indexers.delete'), onClick: remove, disabled: busy } : undefined
        }
      />
    </Modal>
  );
}
