// Generic settings renderer: fetches a settings view (general / network / vpn /
// acquisition / ...) and renders its groups of toggle/select/text/value rows.
// Changes persist immediately (optimistic) via PUT /api/admin/settings. Shared
// by the built-in settings pages AND the VPN / Acquisition module pages.

import type { MessageKey, SettingGroup, SettingRow } from '@kroma/core';
import { useT } from '@kroma/ui';
import { useEffect, useState } from 'react';
import { useAdminKit } from './context';
import { Card, Toggle } from './primitives';
import { Select, TextInput } from './forms';
import { PageHeader } from './header';
import { Denied, useCap } from './hooks';

interface SettingsViewProps {
  view: string;
  titleKey: MessageKey;
  subtitleKey: MessageKey;
  /** Render only the setting groups (no page header), to embed under another
   * page (e.g. the VPN section below its config card). */
  embedded?: boolean;
}

/** Return a copy of `groups` with the row keyed by `key` set to `value` (the rest
 * shared by reference). Hoisted out of the component so the map chain doesn't nest
 * callbacks too deeply. */
function applySetting(groups: SettingGroup[], key: string, value: unknown): SettingGroup[] {
  return groups.map((g) => ({
    ...g,
    rows: g.rows.map((r) => (r.key === key ? { ...r, value } : r)),
  }));
}

export function SettingsView(props: Readonly<SettingsViewProps>) {
  // Settings views require the `settings.manage` capability (server enforces it
  // too); deny cleanly instead of rendering a page that would 403 on every call.
  if (!useCap('settings.manage')) return <Denied />;
  return <SettingsViewInner {...props} />;
}

function SettingsViewInner({ view, titleKey, subtitleKey, embedded }: Readonly<SettingsViewProps>) {
  const t = useT();
  const { client } = useAdminKit();
  const [groups, setGroups] = useState<SettingGroup[]>([]);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    let active = true;
    client
      .adminSettings(view)
      .then((r) => active && setGroups(r.groups))
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [client, view]);

  function set(key: string, value: unknown) {
    setGroups((gs) => applySetting(gs, key, value));
    client
      .updateSettings({ [key]: value })
      .then(() => {
        setSaved(true);
        setTimeout(() => setSaved(false), 1500);
      })
      .catch(() => undefined);
  }

  return (
    <>
      {embedded ? null : (
        <PageHeader
          title={t(titleKey)}
          subtitle={t(subtitleKey)}
          action={
            <span className="shrink-0 text-[13px] font-semibold text-success">
              {saved ? t('admin.saved') : ''}
            </span>
          }
        />
      )}
      <div className={`${embedded ? '' : 'mt-6 '}flex flex-col gap-5.5`}>
        {groups.map((g) => (
          <Card key={g.title} className="overflow-hidden">
            <div className="border-b border-border px-5.5 py-4.25">
              <div className="font-display text-[15px] font-bold">{g.title}</div>
              {g.desc ? <div className="mt-0.75 text-[12.5px] text-dim">{g.desc}</div> : null}
            </div>
            {g.rows.map((r) => (
              <Row key={r.key} row={r} onChange={(v) => set(r.key, v)} />
            ))}
          </Card>
        ))}
      </div>
    </>
  );
}

function Row({ row, onChange }: Readonly<{ row: SettingRow; onChange: (v: unknown) => void }>) {
  const t = useT();
  return (
    <div className="flex flex-wrap items-center justify-between gap-5 border-b border-white/4 px-5.5 py-4 last:border-b-0">
      <div className="min-w-0">
        <div className="text-[14.5px] font-bold">{row.label}</div>
        {row.desc ? <div className="mt-0.75 text-[12.5px] text-dim">{row.desc}</div> : null}
        {!row.applied && row.kind !== 'value' ? (
          <div className="mt-1 text-[11px] font-semibold uppercase tracking-widest text-text/30">
            {t('admin.prefSaved')}
          </div>
        ) : null}
      </div>
      <div className="shrink-0">
        <Control row={row} onChange={onChange} />
      </div>
    </div>
  );
}

/** A setting's stored value (untyped `serde_json::Value`) as editable text; an
 * object would otherwise stringify to "[object Object]". */
function asText(v: unknown): string {
  if (v == null) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return JSON.stringify(v);
}

function Control({ row, onChange }: Readonly<{ row: SettingRow; onChange: (v: unknown) => void }>) {
  if (row.kind === 'toggle') {
    return <Toggle on={Boolean(row.value)} onChange={onChange} />;
  }
  if (row.kind === 'select') {
    return <Select value={asText(row.value)} options={row.options ?? []} onChange={onChange} />;
  }
  if (row.kind === 'text') {
    return <EditableText value={asText(row.value)} onCommit={onChange} />;
  }
  // value (read-only)
  return (
    <span className="text-[13.5px] font-semibold tabular-nums text-text/60">
      {asText(row.value)}
    </span>
  );
}

function EditableText({
  value,
  onCommit,
}: Readonly<{ value: string; onCommit: (v: string) => void }>) {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  return (
    <TextInput
      value={v}
      onChange={setV}
      onBlur={() => {
        if (v !== value) onCommit(v);
      }}
      className="min-w-50"
    />
  );
}
