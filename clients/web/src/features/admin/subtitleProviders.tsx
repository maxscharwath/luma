// One subtitle provider as an expandable card, mirroring the AI provider card.
// The kind selector (OpenSubtitles / Whisper cloud / Whisper local / AI translate)
// swaps which fields show; secrets blank by default keep the stored values.
import type { LumaClient } from '@luma/core';
import { useT } from '@luma/ui';
import {
  IconCheck,
  IconChevronDown,
  IconPlugConnected,
  IconStar,
  IconTrash,
  IconX,
} from '@tabler/icons-react';
import { useState } from 'react';
import { Button, C, Card, Field, Pill, SegmentedControl, TextInput } from '#web/features/admin/ui';

export type SubProviderForm = {
  key: string;
  id: string;
  name: string;
  kind: string;
  baseUrl: string;
  model: string;
  username: string;
  apiKey: string;
  password: string;
  hasApiKey: boolean;
  hasPassword: boolean;
};

const KINDS = [
  { value: 'opensubtitles', label: 'OpenSubtitles' },
  { value: 'whisper', label: 'Whisper · cloud' },
  { value: 'whisperLocal', label: 'Whisper · local' },
  { value: 'translate', label: 'AI translate' },
];
/** Default display name per kind, so a new provider is auto-labelled. */
const KIND_NAME: Record<string, string> = {
  opensubtitles: 'OpenSubtitles',
  whisper: 'Whisper (cloud)',
  whisperLocal: 'Whisper (local)',
  translate: 'AI translate',
};

export function SubProviderCard({
  provider: p,
  isDefault,
  expanded,
  onToggle,
  onChange,
  onSetDefault,
  onRemove,
  whisperAvailable,
  client,
}: Readonly<{
  provider: SubProviderForm;
  isDefault: boolean;
  expanded: boolean;
  onToggle: () => void;
  onChange: (patch: Partial<SubProviderForm>) => void;
  onSetDefault: () => void;
  onRemove: () => void;
  /** Whether the server build includes in-process Whisper (gates that kind). */
  whisperAvailable: boolean;
  client: LumaClient;
}>) {
  const t = useT();
  const [probe, setProbe] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  // Hide local Whisper unless the server build supports it.
  const kinds = KINDS.filter((k) => k.value !== 'whisperLocal' || whisperAvailable);
  const set = (patch: Partial<SubProviderForm>) => {
    onChange(patch);
    setProbe(null);
  };
  // Switching kind: relabel the provider if it still has a default name.
  const setKind = (kind: string) => {
    const renamed = !p.name.trim() || Object.values(KIND_NAME).includes(p.name.trim());
    set({ kind, ...(renamed ? { name: KIND_NAME[kind] ?? p.name } : {}) });
  };
  const test = async () => {
    setBusy(true);
    try {
      const r = await client.testSubtitleProvider({ id: p.id, ...(p.apiKey ? { apiKey: p.apiKey } : {}) });
      setProbe({ ok: r.ok, text: r.message });
    } finally {
      setBusy(false);
    }
  };

  const keyField = (placeholder: string) => (
    <Field label={t('admin.subApiKey')}>
      <TextInput
        value={p.apiKey}
        onChange={(v) => set({ apiKey: v })}
        type="password"
        placeholder={p.hasApiKey ? t('admin.aiApiKeyKeep') : placeholder}
        className="w-full max-w-120 font-mono"
      />
    </Field>
  );

  return (
    <Card className="overflow-hidden">
      <button type="button" onClick={onToggle} className="flex w-full items-center gap-3 px-5 py-4 text-left">
        <span
          className="h-2.5 w-2.5 shrink-0 rounded-full"
          style={{ background: isDefault ? C.accent : 'rgba(255,255,255,.18)' }}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-[14.5px] font-bold">{p.name || t('admin.subUntitled')}</span>
            <Pill color="#9AA0AA" bg="rgba(255,255,255,.06)">
              {KINDS.find((k) => k.value === p.kind)?.label ?? p.kind}
            </Pill>
            {isDefault ? (
              <Pill color={C.accent} bg="rgba(244,182,66,.14)">
                {t('admin.aiDefault')}
              </Pill>
            ) : null}
          </div>
        </div>
        <IconChevronDown
          size={16}
          className={`shrink-0 text-dim transition-transform ${expanded ? 'rotate-180' : ''}`}
        />
      </button>

      {expanded ? (
        <div className="border-t border-border px-5 pt-5">
          <Field label={t('admin.subProviderName')}>
            <TextInput value={p.name} onChange={(v) => set({ name: v })} className="w-full max-w-120" />
          </Field>
          <Field label={t('admin.subKind')} hint={t(`admin.subKind_${p.kind}` as Parameters<typeof t>[0])}>
            <SegmentedControl value={p.kind} onChange={setKind} options={kinds} />
          </Field>

          {p.kind === 'opensubtitles' ? (
            <>
              {keyField('')}
              <Field label={t('admin.osUsername')}>
                <TextInput value={p.username} onChange={(v) => set({ username: v })} className="w-full max-w-120" />
              </Field>
              <Field label={t('admin.osPassword')}>
                <TextInput
                  value={p.password}
                  onChange={(v) => set({ password: v })}
                  type="password"
                  placeholder={p.hasPassword ? t('admin.aiApiKeyKeep') : ''}
                  className="w-full max-w-120 font-mono"
                />
              </Field>
            </>
          ) : null}

          {p.kind === 'whisper' ? (
            <>
              {keyField('sk-…')}
              <Field label={t('admin.aiBaseUrl')} hint={t('admin.subWhisperBaseHint')}>
                <TextInput
                  value={p.baseUrl}
                  onChange={(v) => set({ baseUrl: v })}
                  placeholder="https://api.openai.com/v1"
                  className="w-full max-w-120 font-mono"
                />
              </Field>
              <Field label={t('admin.aiModel')}>
                <TextInput
                  value={p.model}
                  onChange={(v) => set({ model: v })}
                  placeholder="whisper-1"
                  className="w-72 font-mono"
                />
              </Field>
            </>
          ) : null}

          {p.kind === 'whisperLocal' ? (
            <>
              <Field label={t('admin.subBinaryPath')} hint={t('admin.subBinaryHint')}>
                <TextInput
                  value={p.baseUrl}
                  onChange={(v) => set({ baseUrl: v })}
                  placeholder="whisper-cli"
                  className="w-full max-w-120 font-mono"
                />
              </Field>
              <Field label={t('admin.subModelPath')}>
                <TextInput
                  value={p.model}
                  onChange={(v) => set({ model: v })}
                  placeholder="/models/ggml-base.bin"
                  className="w-full max-w-120 font-mono"
                />
              </Field>
            </>
          ) : null}

          {p.kind === 'translate' ? <p className="mb-4 text-[13px] text-dim">{t('admin.subTranslateHint')}</p> : null}

          <div className="mb-5 mt-2 flex flex-wrap items-center gap-2.5">
            {p.kind === 'opensubtitles' ? (
              <Button
                label={busy ? t('admin.aiTesting') : t('admin.aiTest')}
                icon={IconPlugConnected}
                onClick={() => void test()}
                disabled={busy}
              />
            ) : null}
            {!isDefault ? <Button label={t('admin.aiSetDefault')} icon={IconStar} onClick={onSetDefault} /> : null}
            {probe ? (
              <span
                className="inline-flex items-center gap-1.5 text-[13px] font-semibold"
                style={{ color: probe.ok ? C.green : C.red }}
              >
                {probe.ok ? <IconCheck size={15} stroke={2.4} /> : <IconX size={15} stroke={2.4} />}
                {probe.text}
              </span>
            ) : null}
            <button
              type="button"
              onClick={onRemove}
              className="ml-auto inline-flex items-center gap-1.5 text-[13px] font-semibold text-[#E8536A]"
            >
              <IconTrash size={15} stroke={2} />
              {t('admin.aiRemoveProvider')}
            </button>
          </div>
        </div>
      ) : null}
    </Card>
  );
}
