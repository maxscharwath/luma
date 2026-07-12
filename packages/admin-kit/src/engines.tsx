// Data-driven engine add-flows for the admin console. `GET /api/modules` reports
// every module's enabled flag and the capabilities it provides; each engine
// capability (`download-client`, `indexer-engine`, ...) carries an add-form schema
// (`fields`) or a custom `flow` (e.g. the native Cardigann definition picker).
// These helpers turn that into UI: a host page lists one add-flow per enabled
// engine and renders <AddEngineModal> (a generic form over the engine's fields),
// so disabling a module hides its add-UI and adding an engine needs no frontend
// change.

import { apiErrorText, type EngineCapability, type EngineField, type MessageKey } from '@luma/core';
import { useT } from '@luma/ui';
import { useMemo, useState } from 'react';
import { useAdminKit } from './context';
import { SegmentedControl } from './controls';
import { Field, Modal, ModalActions, Select, TextInput } from './forms';
import { useAsyncAction, usePoll } from './hooks';

/** True when a capability actually has an add-flow to render (a plain field form
 * or a custom `flow`). Engines without one (e.g. the always-on embedded `rqbit`)
 * are not offered in the add-picker. */
function hasAddFlow(cap: EngineCapability): boolean {
  return cap.flow != null || (cap.fields?.length ?? 0) > 0;
}

/** The enabled engines that provide `kind` and expose an add-flow. Polls
 * `/api/modules`; a disabled module contributes nothing, so its add-flow
 * disappears from the page. */
export function useEnabledEngines(kind: string): EngineCapability[] {
  const { client } = useAdminKit();
  const { data } = usePoll(['adminModules'], () => client.modules(), 30000);
  return useMemo(
    () =>
      (data ?? [])
        .filter((m) => m.enabled !== false)
        .flatMap((m) => (m.provides ?? []).filter((c) => c.kind === kind && hasAddFlow(c))),
    [data, kind],
  );
}

/** Whether module `id` is enabled. Defaults to true while loading / when unknown,
 * so nothing flickers off before the first poll resolves. */
export function useModuleEnabled(id: string): boolean {
  const { client } = useAdminKit();
  const { data } = usePoll(['adminModules'], () => client.modules(), 30000);
  const mod = (data ?? []).find((m) => m.id === id);
  return mod ? mod.enabled !== false : true;
}

/** A controlled form over an engine's declared fields. Every label resolves
 * through `t()`, so a field key like `field.url` localizes while a proper-noun
 * engine label (`Transmission`) passes through unchanged. */
export function FieldForm({
  fields,
  values,
  onChange,
}: Readonly<{
  fields: EngineField[];
  values: Record<string, string>;
  onChange: (key: string, value: string) => void;
}>) {
  const t = useT();
  return (
    <>
      {fields.map((f) => (
        <Field key={f.key} label={t(f.label as MessageKey)}>
          {f.type === 'select' ? (
            <Select
              value={values[f.key] ?? ''}
              options={f.options ?? []}
              onChange={(v) => onChange(f.key, v)}
            />
          ) : (
            <TextInput
              value={values[f.key] ?? ''}
              onChange={(v) => onChange(f.key, v)}
              type={f.secret ? 'password' : 'text'}
              placeholder={f.placeholder}
              className="w-full"
            />
          )}
        </Field>
      ))}
    </>
  );
}

/** The generic "add an engine" modal: pick an engine (when there is more than
 * one), name it, fill its declared fields, submit. Rendered entirely from what the
 * backend reports, so a new engine needs no bespoke modal. `onSubmit` receives the
 * chosen engine id and the collected values (`name` plus every field key). */
export function AddEngineModal({
  engines,
  title,
  onClose,
  onSubmit,
}: Readonly<{
  engines: EngineCapability[];
  title: string;
  onClose: () => void;
  onSubmit: (engineId: string, values: Record<string, string>) => Promise<void>;
}>) {
  const t = useT();
  const { busy, error, run } = useAsyncAction();
  const [engineId, setEngineId] = useState(engines[0]?.id ?? '');
  const [name, setName] = useState('');
  const [values, setValues] = useState<Record<string, string>>({});

  const engine = engines.find((e) => e.id === engineId) ?? engines[0];
  const fields = engine?.fields ?? [];
  const setField = (key: string, value: string) => setValues((v) => ({ ...v, [key]: value }));

  const missingRequired = fields.some((f) => f.required && !(values[f.key] ?? '').trim());
  const canSubmit = Boolean(engine) && Boolean(name.trim()) && !missingRequired;

  const submit = () =>
    run(
      async () => {
        if (!engine) return;
        await onSubmit(engine.id, { name: name.trim(), ...values });
        onClose();
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  return (
    <Modal title={title} onClose={onClose}>
      {engines.length > 1 ? (
        <div className="mb-4">
          <SegmentedControl
            value={engineId}
            onChange={setEngineId}
            options={engines.map((e) => ({ value: e.id, label: t((e.label ?? e.id) as MessageKey) }))}
          />
        </div>
      ) : null}
      <Field label={t('field.name')}>
        <TextInput value={name} onChange={setName} className="w-full" />
      </Field>
      <FieldForm fields={fields} values={values} onChange={setField} />
      {error ? <p className="mt-1 text-[13px] font-semibold text-[#EF8091]">{error}</p> : null}
      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={submit}
        confirmLabel={busy ? t('common.saving') : t('common.save')}
        busy={busy}
        disabled={!canSubmit}
      />
    </Modal>
  );
}
