// Admin "IA / Intelligence" page: configure the LLM(s) that power personalized,
// auto-named home sections + taste profiles. Register several providers as
// inline cards (see aiProviders.tsx), pick the default used for generation, and
// save the lot. Backed by /api/admin/llm*.
import type { LlmAdminConfig } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconDeviceFloppy, IconPlus, IconSparkles } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { ProviderCard, type ProviderForm } from '#web/features/admin/ai-providers';
import { Denied, PageHeader, useCap } from '#web/features/admin/shell';
import { Button, C, Card, Pill, Section, Toggle } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';
import { EmptyState } from '#web/shared/ui';

/** A provider row in the form. The persisted `id` is owned by the server (blank
 *  for a not-yet-saved provider); `key` is a client-only, ephemeral handle used
 *  for React keys + local identity we never mint provider ids on the client. */
type Row = ProviderForm & { key: string };
type Config = { enabled: boolean; defaultKey: string; providers: Row[] };

// Ephemeral, monotonic local key NOT a provider id (the server assigns those).
let keySeq = 0;
function nextKey(): string {
  keySeq += 1;
  return `row-${keySeq}`;
}

function emptyProvider(): Row {
  return {
    key: nextKey(),
    id: '',
    name: '',
    provider: 'openai',
    baseUrl: '',
    model: '',
    apiKey: '',
    hasApiKey: false,
    temperature: 0.7,
    maxTokens: 900,
    reasoning: false,
  };
}

/** Map the server config into form rows: attach ephemeral keys, clear the
 *  never-returned secrets, and resolve which row is the default by its id. */
function toConfig(c: LlmAdminConfig): Config {
  const providers: Row[] = c.providers.map((p) => ({ ...p, key: nextKey(), apiKey: '' }));
  const def = providers.find((p) => p.id === c.defaultId) ?? providers[0];
  return { enabled: c.enabled, defaultKey: def?.key ?? '', providers };
}

export function AiPage() {
  const t = useT();
  const { client } = useAuth();
  const canManage = useCap('settings.manage');

  const [config, setConfig] = useState<Config | null>(null);
  const [expandedKey, setExpandedKey] = useState<string | null>(null);
  const [busy, setBusy] = useState<'idle' | 'save'>('idle');
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    client
      .adminLlm()
      .then((c) => setConfig(toConfig(c)))
      .catch(() => undefined);
  }, [client]);

  if (!canManage) return <Denied />;
  if (!config) return null;
  const cfg = config;

  const update = (patch: Partial<Config>) => {
    setConfig((c) => (c ? { ...c, ...patch } : c));
    setSaved(false);
  };
  const patchProvider = (key: string, patch: Partial<Row>) =>
    update({ providers: cfg.providers.map((p) => (p.key === key ? { ...p, ...patch } : p)) });

  const addProvider = () => {
    const np = emptyProvider();
    update({
      providers: [...cfg.providers, np],
      defaultKey: cfg.providers.length === 0 ? np.key : cfg.defaultKey,
    });
    setExpandedKey(np.key);
  };
  const removeProvider = (key: string) => {
    const rest = cfg.providers.filter((p) => p.key !== key);
    update({
      providers: rest,
      defaultKey: cfg.defaultKey === key ? (rest[0]?.key ?? '') : cfg.defaultKey,
    });
  };

  const save = async () => {
    setBusy('save');
    setError(null);
    try {
      // The default is identified by index (new rows have no id yet the server
      // assigns one on save).
      const defaultIndex = Math.max(
        0,
        cfg.providers.findIndex((p) => p.key === cfg.defaultKey),
      );
      await client.saveLlm({
        enabled: cfg.enabled,
        defaultIndex,
        providers: cfg.providers.map((p) => ({
          id: p.id,
          name: p.name,
          provider: p.provider,
          baseUrl: p.baseUrl,
          model: p.model,
          temperature: p.temperature,
          maxTokens: p.maxTokens,
          reasoning: p.reasoning,
          ...(p.apiKey ? { apiKey: p.apiKey } : {}),
        })),
      });
      // Re-fetch so the client adopts the server-assigned ids (needed for the
      // per-provider key probe) and the refreshed hasApiKey flags.
      setConfig(toConfig(await client.adminLlm()));
      setSaved(true);
    } catch {
      setError(t('jobs.saveFailed'));
    } finally {
      setBusy('idle');
    }
  };

  return (
    <>
      <PageHeader
        title={t('admin.aiTitle')}
        subtitle={t('admin.aiSub')}
        action={<StatusChip enabled={cfg.enabled} />}
      />

      {/* Global enable */}
      <Card className="mt-6 flex items-center justify-between gap-4 px-5.5 py-4.5">
        <div className="flex items-center gap-3.5">
          <span
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[11px]"
            style={{ background: 'rgba(244,182,66,.16)', color: C.accent }}
          >
            <IconSparkles size={20} stroke={1.8} />
          </span>
          <div>
            <div className="text-[15px] font-bold">{t('admin.aiEnabled')}</div>
            <div className="mt-0.5 text-[12.5px] text-dim">{t('admin.aiEnabledHint')}</div>
          </div>
        </div>
        <Toggle on={cfg.enabled} onChange={(v) => update({ enabled: v })} />
      </Card>

      {/* Providers */}
      <Section
        title={t('admin.aiProviders')}
        right={<Button label={t('admin.aiAddProvider')} icon={IconPlus} onClick={addProvider} />}
      >
        <p className="-mt-2 mb-4 text-[12.5px] text-dim">{t('admin.aiProvidersHint')}</p>
        {cfg.providers.length === 0 ? (
          <EmptyState
            icon={<IconSparkles size={32} stroke={1.5} />}
            title={t('admin.aiNoProviders')}
            action={
              <Button label={t('admin.aiAddProvider')} icon={IconPlus} onClick={addProvider} />
            }
          />
        ) : (
          <div className="flex flex-col gap-3">
            {cfg.providers.map((p) => (
              <ProviderCard
                key={p.key}
                provider={p}
                isDefault={cfg.defaultKey === p.key}
                expanded={expandedKey === p.key}
                onToggle={() => setExpandedKey((x) => (x === p.key ? null : p.key))}
                onChange={(patch) => patchProvider(p.key, patch)}
                onSetDefault={() => update({ defaultKey: p.key })}
                onRemove={() => removeProvider(p.key)}
                client={client}
              />
            ))}
          </div>
        )}
      </Section>

      {/* Save */}
      <div className="mt-6 flex flex-wrap items-center gap-3">
        <Button
          label={busy === 'save' ? t('admin.aiSaving') : t('common.save')}
          icon={IconDeviceFloppy}
          variant="primary"
          onClick={() => void save()}
          disabled={busy !== 'idle'}
        />
        {saved ? (
          <span className="text-[13px] font-semibold" style={{ color: C.green }}>
            {t('admin.aiSaved')}
          </span>
        ) : null}
        {error ? (
          <span className="text-[13px] font-semibold" style={{ color: C.red }}>
            {error}
          </span>
        ) : null}
      </div>
    </>
  );
}

function StatusChip({ enabled }: Readonly<{ enabled: boolean }>) {
  const t = useT();
  return enabled ? (
    <Pill color={C.accent} bg="rgba(244,182,66,.14)">
      {t('admin.aiStatusOn')}
    </Pill>
  ) : (
    <Pill color="#9AA0AA" bg="rgba(255,255,255,.06)">
      {t('admin.aiStatusOff')}
    </Pill>
  );
}
