// The VPN module page (`/admin/m/vpn`): the managed WireGuard bridge state, a
// paste-your-config modal and a live seal test, plus the network-wide toggles
// (kill switch, route indexers through the tunnel) from the settings view. VPN
// routing is WireGuard-only (any provider). Default export so the module runtime
// can React.lazy it into its own chunk.

import {
  apiErrorText,
  Card,
  Denied,
  Modal,
  ModalActions,
  PageHeader,
  Pill,
  SettingsView,
  useAdminKit,
  useAsyncAction,
  useCap,
  usePoll,
  useT,
  type VpnTestResult,
} from '@kroma/module-sdk';
import { IconLoader2, IconShield, IconShieldCheck, IconShieldX } from '@tabler/icons-react';
import { useState } from 'react';

// The VPN is global to several flows (torrent downloads and, optionally, indexer
// searches), so it lives on its own page: the WireGuard config card + the
// network-wide toggles (kill switch, route indexers through the tunnel).
export default function VpnPage() {
  const t = useT();
  if (!useCap('settings.manage')) return <Denied />;
  return (
    <>
      <PageHeader title={t('admin.vpnTitle')} subtitle={t('admin.vpnSub')} />
      <div className="mt-6" />
      <VpnCard />
      <SettingsView view="vpn" titleKey="admin.vpnTitle" subtitleKey="admin.vpnSub" embedded />
    </>
  );
}

export function VpnCard() {
  const t = useT();
  const { client } = useAdminKit();
  const [modal, setModal] = useState(false);
  const [test, setTest] = useState<{ busy?: boolean; result?: VpnTestResult; error?: string }>({});
  const { data, reload } = usePoll(['admin', 'vpn'], () => client.adminVpn(), 30000);

  const runTest = () => {
    setTest({ busy: true });
    client
      .testVpn()
      .then((result) => setTest({ result }))
      .catch((e) => setTest({ error: apiErrorText(e, t('vpn.testFailed')) }));
  };

  const state = data;
  const configured = state?.wgConfigured ?? false;
  const connected = state?.status?.connected ?? false;

  return (
    <Card className="mb-5 p-4.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3">
          <span
            className="flex h-10 w-10 flex-[0_0_40px] items-center justify-center rounded-xl border border-border-strong bg-surface-2"
            style={{ color: statusColor(configured, connected) }}
          >
            <StatusIcon configured={configured} connected={connected} />
          </span>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-[14.5px] font-bold">{t('vpn.title')}</span>
              {configured ? <BridgePill running={state?.bridgeRunning ?? false} /> : null}
            </div>
            <div className="mt-0.5 text-[12px] font-medium text-dim">
              {configured
                ? t('vpn.modeWireguard', { port: String(state?.localPort ?? 0) })
                : t('vpn.modeOff')}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {configured ? (
            <button
              type="button"
              onClick={runTest}
              disabled={test.busy}
              className="inline-flex items-center gap-1.5 rounded-lg border border-white/12 bg-[#1A1A20] px-3 py-2 text-[12.5px] font-semibold text-white/80 hover:bg-[#222229] disabled:opacity-60"
            >
              {test.busy ? <IconLoader2 size={13} stroke={2.4} className="animate-spin" /> : null}
              {t('vpn.test')}
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => setModal(true)}
            className="rounded-lg bg-accent px-3.5 py-2 text-[12.5px] font-bold text-accent-ink hover:bg-accent-hover"
          >
            {t(state?.wgConfigured ? 'vpn.reconfigure' : 'vpn.configure')}
          </button>
        </div>
      </div>

      {test.error || test.result ? (
        <div className="mt-3 border-t border-white/6 pt-3 text-[12.5px] font-semibold">
          <TestResultLine test={test} />
        </div>
      ) : null}

      {modal ? (
        <VpnConfigModal
          configured={configured}
          onClose={() => setModal(false)}
          onSaved={() => {
            reload();
            setTest({});
          }}
        />
      ) : null}
    </Card>
  );
}

function StatusIcon({
  configured,
  connected,
}: Readonly<{ configured: boolean; connected: boolean }>) {
  if (!configured) return <IconShield size={18} stroke={1.8} />;
  if (connected) return <IconShieldCheck size={18} stroke={1.8} />;
  return <IconShieldX size={18} stroke={1.8} />;
}

function statusColor(configured: boolean, connected: boolean): string {
  if (!configured) return 'rgba(244,243,240,.5)';
  return connected ? '#46D08D' : '#F4B642';
}

function BridgePill({ running }: Readonly<{ running: boolean }>) {
  const t = useT();
  return (
    <Pill color={running ? '#46D08D' : '#E8536A'}>
      {running ? t('vpn.bridgeUp') : t('vpn.bridgeDown')}
    </Pill>
  );
}

function TestResultLine({
  test,
}: Readonly<{ test: { busy?: boolean; result?: VpnTestResult; error?: string } }>) {
  const t = useT();
  if (test.error) return <span className="text-[#EF8091]">{test.error}</span>;
  if (test.result?.sealed) {
    return (
      <span className="text-[#46D08D]">
        {t('vpn.sealed', { ip: test.result.proxiedIp ?? '?' })}
        {test.result.directIp ? ` · ${t('vpn.directIp', { ip: test.result.directIp })}` : ''}
      </span>
    );
  }
  return <span className="text-[#F4B642]">{test.result?.error ?? t('vpn.notSealed')}</span>;
}

function VpnConfigModal({
  configured,
  onClose,
  onSaved,
}: Readonly<{ configured: boolean; onClose: () => void; onSaved: () => void }>) {
  const t = useT();
  const { client } = useAdminKit();
  const { busy, error, run } = useAsyncAction();
  const [config, setConfig] = useState('');

  const save = (wgConfig: string) =>
    run(
      async () => {
        await client.saveVpn({ wgConfig, localPort: null });
        onSaved();
        onClose();
      },
      (e) => apiErrorText(e, t('requests.actionFailed')),
    );

  return (
    <Modal title={t('vpn.modalTitle')} onClose={onClose}>
      <p className="mb-3 text-[13px] leading-relaxed text-dim">{t('vpn.modalHelp')}</p>
      <textarea
        value={config}
        onChange={(e) => setConfig(e.target.value)}
        placeholder={
          '[Interface]\nPrivateKey = ...\nAddress = 10.2.0.2/32\n\n[Peer]\nPublicKey = ...\nEndpoint = ...:51820\nAllowedIPs = 0.0.0.0/0'
        }
        rows={9}
        className="w-full rounded-[9px] border border-border-strong bg-[#0F0F13] px-3.5 py-2.5 font-mono text-[12px] leading-relaxed text-text outline-none placeholder:text-white/25 focus:border-accent/60"
      />
      {configured ? <p className="mt-2 text-[12px] text-dim">{t('vpn.configKept')}</p> : null}
      {error ? <p className="mt-2 text-[13px] font-semibold text-[#EF8091]">{error}</p> : null}
      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={() => save(config.trim())}
        confirmLabel={busy ? t('common.saving') : t('common.save')}
        busy={busy}
        disabled={!config.trim()}
        destructive={
          configured
            ? { label: t('vpn.removeConfig'), onClick: () => save(''), disabled: busy }
            : undefined
        }
      />
    </Modal>
  );
}
