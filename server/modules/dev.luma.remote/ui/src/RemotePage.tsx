// Admin "Remote access" page: configure the public URL used for share / Quick
// Connect links, and (optionally) let LUMA run + supervise a Cloudflare Tunnel
// `cloudflared` connector so a box with no existing tunnel gets a public HTTPS
// endpoint without port-forwarding. Backed by /api/admin/remote.
//
// One control drives the connector: the enable toggle (auto-saved). The server
// reconciles the running connector to match it, so disabling always stops it.
import type { RemoteAccessView } from '@luma/module-sdk';
import {
  Button,
  C,
  Card,
  Denied,
  Field,
  PageHeader,
  Pill,
  Section,
  TextInput,
  Toggle,
  useAdminKit,
  useCap,
} from '@luma/module-sdk';
import { useT } from '@luma/module-sdk';
import { IconCloud, IconDeviceFloppy, IconExternalLink } from '@tabler/icons-react';
import { useEffect, useRef, useState } from 'react';

// Deep link to the Zero Trust "Tunnels" page (`:account` auto-resolves to the
// signed-in account) where a tunnel's connector token is created and shown in the
// `cloudflared … run --token <TOKEN>` command.
const CF_TUNNELS_URL = 'https://one.dash.cloudflare.com/?to=/:account/networks/tunnels';

export default function RemotePage() {
  const t = useT();
  const { client } = useAdminKit();
  const canManage = useCap('settings.manage');

  // Server view is the source of truth for live status + `hasToken`; the form
  // fields are editable copies so polling never clobbers in-progress edits.
  const [view, setView] = useState<RemoteAccessView | null>(null);
  const [url, setUrl] = useState('');
  const [enabled, setEnabled] = useState(false);
  const [token, setToken] = useState('');
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const loaded = useRef(false);

  useEffect(() => {
    client
      .adminRemote()
      .then((v) => {
        setView(v);
        if (!loaded.current) {
          loaded.current = true;
          setUrl(v.url);
          setEnabled(v.enabled);
        }
      })
      .catch(() => undefined);
  }, [client]);

  // Poll live status (running / logs) without touching the form fields.
  useEffect(() => {
    const id = setInterval(() => {
      client
        .adminRemote()
        .then(setView)
        .catch(() => undefined);
    }, 4000);
    return () => clearInterval(id);
  }, [client]);

  if (!canManage) return <Denied />;
  if (!view) return null;
  const st = view.status;

  // Persist config; the server reconciles the connector to match `enabled`.
  const persist = async (en: boolean) => {
    setBusy(true);
    setSaved(false);
    try {
      const v = await client.saveRemote({ enabled: en, url, ...(token ? { token } : {}) });
      setView(v);
      if (token) setToken('');
      setSaved(true);
    } finally {
      setBusy(false);
    }
  };
  // The toggle is the single connector control it auto-saves so enabling /
  // disabling takes effect immediately (and survives a restart).
  const toggle = (v: boolean) => {
    setEnabled(v);
    void persist(v);
  };

  return (
    <>
      <PageHeader
        title={t('admin.remoteAccess')}
        subtitle={t('admin.remoteAccessDesc')}
        action={<StatusChip status={st} />}
      />

      {/* Public URL (used for share / Quick Connect links; always applicable). */}
      <Card className="mt-6 px-5.5 py-5">
        <Field label={t('admin.customUrl')} hint={t('admin.customUrlHint')}>
          <TextInput
            value={url}
            onChange={setUrl}
            placeholder="https://luma.example.com"
            className="w-full"
          />
        </Field>
      </Card>

      {/* Managed connector (optional). */}
      <Section title={t('admin.remoteManaged')}>
        <p className="-mt-2 mb-4 text-[12.5px] text-dim">{t('admin.remoteManagedHint')}</p>
        <Card className="px-5.5 py-5">
          <div className="mb-4 flex items-center justify-between gap-4">
            <div className="flex items-center gap-3.5">
              <span
                className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[11px]"
                style={{ background: 'rgba(92,141,246,.16)', color: C.blue }}
              >
                <IconCloud size={20} stroke={1.8} />
              </span>
              <div>
                <div className="text-[15px] font-bold">{t('admin.enableRemoteAccess')}</div>
                <div className="mt-0.5 text-[12.5px] text-dim">{t('admin.remoteAccessDesc')}</div>
              </div>
            </div>
            <Toggle on={enabled} onChange={toggle} />
          </div>

          <Field label={t('admin.remoteToken')} hint={t('admin.remoteTokenHint')}>
            <TextInput
              value={token}
              onChange={setToken}
              type="password"
              placeholder={view.hasToken ? t('admin.remoteTokenKeep') : 'eyJhIjoi…'}
              className="w-full"
            />
          </Field>

          <a
            href={CF_TUNNELS_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="mb-4 inline-flex items-center gap-1.5 text-[12.5px] font-semibold text-accent hover:underline"
          >
            <IconExternalLink size={13} stroke={2} />
            {t('admin.remoteGetToken')}
          </a>

          <div className="mt-1 flex flex-wrap items-center gap-3">
            <Button
              label={busy ? t('admin.aiSaving') : t('common.save')}
              icon={IconDeviceFloppy}
              variant="primary"
              onClick={() => void persist(enabled)}
              disabled={busy}
            />
            {saved ? (
              <span className="text-[13px] font-semibold" style={{ color: C.green }}>
                {t('admin.remoteSaved')}
              </span>
            ) : null}
          </div>
        </Card>
      </Section>

      {/* Live connector status + logs. */}
      <Section title={t('admin.remoteLogs')}>
        <Card className="px-5.5 py-5">
          <div className="flex flex-wrap items-center gap-x-6 gap-y-1.5 text-[13px]">
            <StatusChip status={st} />
            {st.since ? (
              <span className="text-dim">
                {t('admin.remoteSince')} {new Date(st.since).toLocaleString()}
              </span>
            ) : null}
            {st.binaryFound ? (
              <span className="text-dim">{st.binaryVersion ?? 'cloudflared'}</span>
            ) : (
              <span style={{ color: C.red }}>{t('admin.remoteBinaryMissing')}</span>
            )}
          </div>
          {st.lastError ? (
            <div className="mt-2 text-[12.5px]" style={{ color: C.red }}>
              {st.lastError}
            </div>
          ) : null}
          <pre className="mt-3 max-h-72 overflow-auto rounded-[9px] border border-border bg-[#0B0B0E] p-3 text-[11.5px] leading-relaxed text-muted">
            {st.logs.length ? st.logs.join('\n') : t('admin.remoteNoLogs')}
          </pre>
        </Card>
      </Section>
    </>
  );
}

function StatusChip({ status }: Readonly<{ status: RemoteAccessView['status'] }>) {
  const t = useT();
  if (status.running) {
    return (
      <Pill color={C.green} bg="rgba(70,208,141,.14)">
        {t('admin.remoteConnected')}
      </Pill>
    );
  }
  if (status.connecting) {
    return (
      <Pill color={C.accent} bg="rgba(244,182,66,.14)">
        {t('admin.remoteConnecting')}
      </Pill>
    );
  }
  return (
    <Pill color="#9AA0AA" bg="rgba(255,255,255,.06)">
      {t('admin.remoteDisconnected')}
    </Pill>
  );
}
