// Typed settings form for a module's declared `config` fields. Renders the right
// control per field kind (text / number / checkbox / select) and PUTs properly
// typed JSON values to /api/admin/modules/:id/config (bool and number, not the
// stringified values the old text-only form sent).

import type { ConfigField } from '@kroma/module-sdk';
import { type ReactNode, useId, useState } from 'react';
import { adminApi } from '#web/features/admin/module-api';
import { Toggle } from '#web/features/admin/ui';

type ConfigValue = string | number | boolean;

/** Seed a field's editable value from the stored value (which may already be
 *  typed) or its string `default`, coerced to the field's kind. */
function initial(field: ConfigField, stored: unknown): ConfigValue {
  const raw = stored ?? field.default;
  switch (field.type) {
    case 'bool':
      return raw === true || raw === 'true';
    case 'number': {
      const n = typeof raw === 'number' ? raw : Number(raw);
      return Number.isFinite(n) ? n : 0;
    }
    default:
      return raw == null ? '' : String(raw);
  }
}

export function ModuleConfigForm({
  moduleId,
  fields,
  values,
  onSaved,
}: Readonly<{
  moduleId: string;
  fields: ConfigField[];
  values: Record<string, unknown>;
  onSaved: () => void;
}>) {
  const [draft, setDraft] = useState<Record<string, ConfigValue>>(() =>
    Object.fromEntries(fields.map((f) => [f.key, initial(f, values[f.key])])),
  );
  const [saving, setSaving] = useState(false);
  const set = (key: string, v: ConfigValue) => setDraft((d) => ({ ...d, [key]: v }));

  const save = async () => {
    setSaving(true);
    try {
      await adminApi(`/modules/${encodeURIComponent(moduleId)}/config`, {
        method: 'PUT',
        body: JSON.stringify(draft),
      });
      onSaved();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mt-3 flex flex-col gap-2 border-t border-border pt-3">
      {fields.map((f) => (
        <Field key={f.key} field={f} value={draft[f.key]} onChange={(v) => set(f.key, v)} />
      ))}
      <button
        type="button"
        onClick={() => void save()}
        disabled={saving}
        className="self-end rounded bg-accent-soft px-3 py-1 text-xs font-semibold text-accent disabled:opacity-50"
      >
        {saving ? 'Saving...' : 'Save'}
      </button>
    </div>
  );
}

function Field({
  field,
  value,
  onChange,
}: Readonly<{ field: ConfigField; value: ConfigValue | undefined; onChange: (v: ConfigValue) => void }>) {
  const id = useId();
  const inputCls = 'w-40 rounded border border-border bg-transparent px-2 py-1 text-text';

  let control: ReactNode;
  if (field.type === 'bool') {
    control = <Toggle on={value === true} onChange={onChange} />;
  } else if (field.type === 'select') {
    control = (
      <select
        id={id}
        className={inputCls}
        value={String(value ?? '')}
        onChange={(e) => onChange(e.target.value)}
      >
        {(field.options ?? []).map((opt) => (
          <option key={opt} value={opt}>
            {opt}
          </option>
        ))}
      </select>
    );
  } else if (field.type === 'number') {
    control = (
      <input
        id={id}
        type="number"
        className={inputCls}
        value={typeof value === 'number' ? value : ''}
        onChange={(e) => onChange(e.target.value === '' ? 0 : Number(e.target.value))}
      />
    );
  } else {
    control = (
      <input
        id={id}
        className={inputCls}
        value={String(value ?? '')}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  }

  return (
    <label htmlFor={id} className="flex items-center justify-between gap-2 text-xs">
      <span className="text-muted">{field.label}</span>
      {control}
    </label>
  );
}
