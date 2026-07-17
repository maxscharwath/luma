// One LLM provider, rendered as an inline expandable card (mirrors the admin
// Libraries pattern): collapsed header shows name + type + default badge +
// model·host; expanded reveals the editable fields (provider type, base URL,
// API key, searchable model picker, advanced) and per-card Test / Set default /
// Remove. Backed by /api/admin/llm* each card probes its own in-progress values.
import type { KromaClient, MessageKey } from '@kroma/core';
import { useT } from '@kroma/ui';
import {
  IconCheck,
  IconChevronDown,
  IconPlugConnected,
  IconReload,
  IconStar,
  IconTrash,
  IconX,
} from '@tabler/icons-react';
import { useState } from 'react';
import {
  Button,
  C,
  Card,
  Disclosure,
  Field,
  NumberField,
  Pill,
  SegmentedControl,
  TextInput,
  Toggle,
} from '#web/features/admin/ui';
import { SearchSelect } from './search-select';

/** Editable provider the view fields plus a transient `apiKey` ('' = keep the
 *  stored secret) and `hasApiKey` (whether one is stored server-side). */
export type ProviderForm = {
  id: string;
  name: string;
  provider: string;
  baseUrl: string;
  model: string;
  apiKey: string;
  hasApiKey: boolean;
  temperature: number;
  maxTokens: number;
  reasoning: boolean;
};

/** Default base URL per provider (blank = the user must supply one). */
const PROVIDER_BASE: Record<string, string> = {
  openrouter: 'https://openrouter.ai/api/v1',
  anthropic: '',
  openai: '',
};
const BASE_HINT_KEY: Record<string, MessageKey> = {
  anthropic: 'admin.aiBaseUrlAnthropic',
  openrouter: 'admin.aiBaseUrlOpenrouter',
};
const MODEL_PLACEHOLDER: Record<string, string> = {
  anthropic: 'claude-haiku-4-5',
  openrouter: 'qwen/qwen-2.5-7b-instruct',
};

/** Per-provider field layout each provider exposes a different set of settings,
 *  so the form adapts: where the base URL lives, whether a key is required, and
 *  which generation controls apply (temperature is OpenAI-only; reasoning is
 *  Anthropic-only). Unknown providers fall back to the openai layout. */
type Spec = {
  baseUrl: 'required' | 'advanced';
  apiKey: 'required' | 'optional';
  temperature: boolean;
  reasoning: boolean;
};
const SPEC_OPENAI: Spec = {
  baseUrl: 'required',
  apiKey: 'optional',
  temperature: true,
  reasoning: false,
};
const SPEC: Record<string, Spec> = {
  openai: SPEC_OPENAI,
  openrouter: { baseUrl: 'advanced', apiKey: 'required', temperature: true, reasoning: false },
  anthropic: { baseUrl: 'advanced', apiKey: 'required', temperature: false, reasoning: true },
};

function hostOf(baseUrl: string, isAnthropic: boolean): string {
  if (!baseUrl) return isAnthropic ? 'api.anthropic.com' : '';
  try {
    return new URL(baseUrl).host;
  } catch {
    return baseUrl;
  }
}

export function ProviderCard({
  provider: p,
  isDefault,
  expanded,
  onToggle,
  onChange,
  onSetDefault,
  onRemove,
  client,
}: Readonly<{
  provider: ProviderForm;
  isDefault: boolean;
  expanded: boolean;
  onToggle: () => void;
  onChange: (patch: Partial<ProviderForm>) => void;
  onSetDefault: () => void;
  onRemove: () => void;
  client: KromaClient;
}>) {
  const t = useT();
  const [models, setModels] = useState<string[]>([]);
  const [busy, setBusy] = useState<'idle' | 'test' | 'models'>('idle');
  const [probe, setProbe] = useState<{ ok: boolean; text: string } | null>(null);

  const isAnthropic = p.provider === 'anthropic';
  const spec = SPEC[p.provider] ?? SPEC_OPENAI;
  const modelPlaceholder = MODEL_PLACEHOLDER[p.provider] ?? 'qwen2.5:1.5b-instruct';

  // Probe with the in-progress values; send the provider id so a blank key falls
  // back to *this* provider's stored secret server-side (omit the key when blank).
  const probeBody = () => ({
    id: p.id,
    provider: p.provider,
    baseUrl: p.baseUrl,
    model: p.model,
    ...(p.apiKey ? { apiKey: p.apiKey } : {}),
  });

  const set = (patch: Partial<ProviderForm>) => {
    onChange(patch);
    setProbe(null);
  };
  // Switching provider points at a different endpoint: reset base URL + models.
  const setProvider = (v: string) => {
    set({ provider: v, baseUrl: PROVIDER_BASE[v] ?? '' });
    setModels([]);
  };

  const loadModels = async () => {
    setBusy('models');
    try {
      const r = await client.llmModels(probeBody());
      setModels(r.models);
      if (r.error) setProbe({ ok: false, text: r.error });
    } finally {
      setBusy('idle');
    }
  };
  const test = async () => {
    setBusy('test');
    try {
      const r = await client.testLlm(probeBody());
      setProbe({ ok: r.ok, text: r.message });
    } finally {
      setBusy('idle');
    }
  };

  const host = hostOf(p.baseUrl, isAnthropic);

  // Placed in the main column (openai) or under Advanced (openrouter/anthropic).
  const baseUrlField = (
    <Field
      label={t('admin.aiBaseUrl')}
      hint={t(BASE_HINT_KEY[p.provider] ?? 'admin.aiBaseUrlHint')}
    >
      <TextInput
        value={p.baseUrl}
        onChange={(v) => set({ baseUrl: v })}
        placeholder={PROVIDER_BASE[p.provider] || 'http://localhost:11434/v1'}
        className="w-full max-w-120 font-mono"
      />
    </Field>
  );

  return (
    <Card className="overflow-hidden">
      {/* Collapsed header click to expand */}
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center gap-3 px-5 py-4 text-left"
      >
        <span
          className="h-2.5 w-2.5 shrink-0 rounded-full"
          style={{ background: isDefault ? C.accent : 'rgba(255,255,255,.18)' }}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-[14.5px] font-bold">
              {p.name || t('admin.aiUntitledProvider')}
            </span>
            <Pill color="#9AA0AA" bg="rgba(255,255,255,.06)">
              {p.provider}
            </Pill>
            {isDefault ? (
              <Pill color={C.accent} bg="rgba(244,182,66,.14)">
                {t('admin.aiDefault')}
              </Pill>
            ) : null}
          </div>
          <div className="mt-0.5 truncate text-[12.5px] text-dim">
            {p.model || '-'}
            {host ? ` · ${host}` : ''}
          </div>
        </div>
        {probe ? (
          <span
            className="h-2 w-2 shrink-0 rounded-full"
            style={{ background: probe.ok ? C.green : C.red }}
          />
        ) : null}
        <IconChevronDown
          size={16}
          className={`shrink-0 text-dim transition-transform ${expanded ? 'rotate-180' : ''}`}
        />
      </button>

      {expanded ? (
        <div className="border-t border-border px-5 pt-5">
          <Field label={t('admin.aiProviderName')}>
            <TextInput
              value={p.name}
              onChange={(v) => set({ name: v })}
              placeholder={t('admin.aiProviderNamePlaceholder')}
              className="w-full max-w-120"
            />
          </Field>

          <Field label={t('admin.aiProvider')} hint={t('admin.aiProviderHint')}>
            <SegmentedControl
              value={p.provider}
              onChange={setProvider}
              options={[
                { value: 'openai', label: t('admin.aiProviderOpenai') },
                { value: 'openrouter', label: t('admin.aiProviderOpenrouter') },
                { value: 'anthropic', label: t('admin.aiProviderAnthropic') },
              ]}
            />
          </Field>

          {spec.baseUrl === 'required' ? baseUrlField : null}

          <Field
            label={`${t('admin.aiApiKey')} · ${spec.apiKey === 'required' ? t('admin.aiRequired') : t('admin.aiOptional')}`}
            hint={t('admin.aiApiKeyHint')}
          >
            <TextInput
              value={p.apiKey}
              onChange={(v) => set({ apiKey: v })}
              type="password"
              placeholder={p.hasApiKey ? t('admin.aiApiKeyKeep') : 'sk-…'}
              className="w-full max-w-120 font-mono"
            />
          </Field>

          <Field
            label={t('admin.aiModel')}
            hint={
              models.length > 0
                ? t('admin.aiModelsCount', { count: models.length })
                : t('admin.aiModelHint')
            }
          >
            <div className="flex flex-wrap items-center gap-2">
              {models.length > 0 ? (
                <SearchSelect
                  value={p.model}
                  options={models}
                  onChange={(v) => set({ model: v })}
                  placeholder={modelPlaceholder}
                  searchPlaceholder={t('admin.aiSearchModels')}
                  className="w-72 max-w-full"
                />
              ) : (
                <TextInput
                  value={p.model}
                  onChange={(v) => set({ model: v })}
                  placeholder={modelPlaceholder}
                  className="w-72 max-w-full font-mono"
                />
              )}
              <Button
                label={busy === 'models' ? t('admin.aiLoading') : t('admin.aiLoadModels')}
                icon={IconReload}
                onClick={loadModels}
                disabled={busy !== 'idle'}
              />
            </div>
          </Field>

          <Disclosure title={t('admin.aiAdvanced')}>
            {spec.baseUrl === 'advanced' ? baseUrlField : null}
            {spec.temperature ? (
              <Field label={t('admin.aiTemperature')} hint={t('admin.aiTemperatureHint')}>
                <NumberField
                  value={p.temperature}
                  step={0.1}
                  min={0}
                  max={2}
                  onChange={(n) => set({ temperature: n })}
                />
              </Field>
            ) : null}
            <Field label={t('admin.aiMaxTokens')} hint={t('admin.aiMaxTokensHint')}>
              <NumberField
                value={p.maxTokens}
                step={100}
                min={64}
                onChange={(n) => set({ maxTokens: n })}
              />
            </Field>
            {spec.reasoning ? (
              <div className="mb-4 flex items-center justify-between gap-4">
                <div>
                  <div className="text-[14px] font-bold">{t('admin.aiReasoning')}</div>
                  <div className="mt-0.5 text-[12.5px] text-dim">{t('admin.aiReasoningHint')}</div>
                </div>
                <Toggle on={p.reasoning} onChange={(v) => set({ reasoning: v })} />
              </div>
            ) : null}
          </Disclosure>

          {/* Per-card actions */}
          <div className="mb-5 mt-2 flex flex-wrap items-center gap-2.5">
            <Button
              label={busy === 'test' ? t('admin.aiTesting') : t('admin.aiTest')}
              icon={IconPlugConnected}
              onClick={() => void test()}
              disabled={busy !== 'idle'}
            />
            {!isDefault ? (
              <Button label={t('admin.aiSetDefault')} icon={IconStar} onClick={onSetDefault} />
            ) : null}
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
