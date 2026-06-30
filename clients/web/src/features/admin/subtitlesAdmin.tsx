// Admin "Subtitles" page: manage subtitle providers (OpenSubtitles + the AI
// engines, Whisper transcription and LLM translation) as inline cards, pick the
// default, and save. Mirrors the AI page; backed by /api/admin/subtitles*.
import { apiErrorText, type SubtitleProvidersConfig } from '@luma/core';
import { useT } from '@luma/ui';
import { IconDeviceFloppy, IconPlus } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { Denied, PageHeader, useCap } from '#web/features/admin/shell';
import { SubProviderCard, type SubProviderForm } from '#web/features/admin/subtitleProviders';
import { Button, C, Card, Section } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';

type Row = SubProviderForm;
type Config = { defaultKey: string; providers: Row[]; whisperLocal: boolean };

let keySeq = 0;
const nextKey = () => `srow-${(keySeq += 1)}`;

function emptyProvider(): Row {
  return {
    key: nextKey(),
    id: '',
    name: '',
    kind: 'opensubtitles',
    baseUrl: '',
    model: '',
    username: '',
    apiKey: '',
    password: '',
    hasApiKey: false,
    hasPassword: false,
  };
}

function toConfig(c: SubtitleProvidersConfig): Config {
  const providers: Row[] = c.providers.map((p) => ({ ...p, key: nextKey(), apiKey: '', password: '' }));
  const def = providers.find((p) => p.id === c.defaultId) ?? providers[0];
  return { defaultKey: def?.key ?? '', providers, whisperLocal: c.whisperLocal };
}

export function SubtitlesPage() {
  const t = useT();
  const { client } = useAuth();
  const canManage = useCap('settings.manage');

  const [config, setConfig] = useState<Config | null>(null);
  const [expandedKey, setExpandedKey] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    client
      .adminSubtitles()
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
    update({ providers: rest, defaultKey: cfg.defaultKey === key ? (rest[0]?.key ?? '') : cfg.defaultKey });
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const defaultIndex = Math.max(
        0,
        cfg.providers.findIndex((p) => p.key === cfg.defaultKey),
      );
      await client.saveSubtitleProviders({
        defaultIndex,
        providers: cfg.providers.map((p) => ({
          id: p.id,
          name: p.name,
          kind: p.kind,
          baseUrl: p.baseUrl,
          model: p.model,
          username: p.username,
          ...(p.apiKey ? { apiKey: p.apiKey } : {}),
          ...(p.password ? { password: p.password } : {}),
        })),
      });
      setConfig(toConfig(await client.adminSubtitles()));
      setSaved(true);
    } catch (e: unknown) {
      setError(apiErrorText(e, t('jobs.saveFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <PageHeader title={t('admin.subTitle')} subtitle={t('admin.subSub')} />
      <Section
        title={t('admin.aiProviders')}
        right={<Button label={t('admin.aiAddProvider')} icon={IconPlus} onClick={addProvider} />}
      >
        <p className="-mt-2 mb-4 text-[12.5px] text-dim">{t('admin.subProvidersHint')}</p>
        {cfg.providers.length === 0 ? (
          <Card className="px-5 py-8 text-center text-[13px] text-dim">{t('admin.subNoProviders')}</Card>
        ) : (
          <div className="flex flex-col gap-3">
            {cfg.providers.map((p) => (
              <SubProviderCard
                key={p.key}
                provider={p}
                isDefault={cfg.defaultKey === p.key}
                expanded={expandedKey === p.key}
                onToggle={() => setExpandedKey((x) => (x === p.key ? null : p.key))}
                onChange={(patch) => patchProvider(p.key, patch)}
                onSetDefault={() => update({ defaultKey: p.key })}
                onRemove={() => removeProvider(p.key)}
                whisperAvailable={cfg.whisperLocal}
                client={client}
              />
            ))}
          </div>
        )}
      </Section>
      <div className="mt-6 flex flex-wrap items-center gap-3">
        <Button
          label={busy ? t('admin.aiSaving') : t('common.save')}
          icon={IconDeviceFloppy}
          variant="primary"
          onClick={() => void save()}
          disabled={busy}
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
