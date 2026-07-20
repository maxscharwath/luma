// Passkeys section ("Clés d'accès"): register WebAuthn credentials for
// passwordless sign-in, list them, and remove them. WebAuthn only works in a
// secure context (HTTPS or localhost), so on plain-HTTP LAN access the card
// shows a notice instead of the add button.

import { apiErrorText, type PasskeyInfo } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconKey, IconPlus, IconShieldLock } from '@tabler/icons-react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { deviceInfo } from '#web/features/accounts/account/sessions-card';
import { Panel } from '#web/features/accounts/account/ui';
import { relativeSeen } from '#web/shared/lib/adminFormat';
import { kromaClient } from '#web/shared/lib/api';
import { userQueries } from '#web/shared/lib/queries';
import { createPasskey, passkeysSupported } from '#web/shared/lib/webauthn';
import { Button } from '#web/shared/ui';

/** A DOMException raised because the user dismissed the browser prompt (not a
 * real failure worth surfacing). */
function isCancel(e: unknown): boolean {
  return e instanceof DOMException && (e.name === 'NotAllowedError' || e.name === 'AbortError');
}

/** Human-facing text for a failed ceremony. WebAuthn failures are `DOMException`s
 * whose `name` (e.g. `SecurityError` for an invalid RP id like an IP address) is
 * the actionable part, so surface it alongside the generic message. */
function ceremonyError(e: unknown, fallback: string): string {
  if (e instanceof DOMException) return `${fallback} (${e.name})`;
  return apiErrorText(e, fallback);
}

function PasskeyRow({
  passkey,
  onRemoved,
}: Readonly<{ passkey: PasskeyInfo; onRemoved: () => void }>) {
  const t = useT();
  const [removing, setRemoving] = useState(false);

  const remove = async () => {
    setRemoving(true);
    try {
      await kromaClient().deletePasskey(passkey.id);
      onRemoved();
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div className="flex items-center justify-between gap-4 px-5.5 py-3.5">
      <div className="flex min-w-0 items-center gap-3.5">
        <span className="flex size-9.5 flex-none items-center justify-center rounded-md border border-border bg-surface-2 text-success">
          <IconKey size={18} stroke={1.7} />
        </span>
        <div className="min-w-0">
          <div className="truncate text-[14px] font-bold text-text">{passkey.name}</div>
          <div className="mt-0.5 truncate text-[12.5px] text-muted">
            {passkey.lastUsed ? relativeSeen(passkey.lastUsed) : t('account.passkeyNeverUsed')}
          </div>
        </div>
      </div>
      <Button
        variant="ghost"
        size="sm"
        onClick={remove}
        disabled={removing}
        className="text-danger"
      >
        {removing ? t('common.saving') : t('common.delete')}
      </Button>
    </div>
  );
}

export function PasskeysCard() {
  const t = useT();
  const qc = useQueryClient();
  const supported = passkeysSupported();
  const { data: keys } = useQuery({ ...userQueries.passkeys(), enabled: supported });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const invalidate = () => qc.invalidateQueries({ queryKey: ['passkeys'] });

  const add = async () => {
    setBusy(true);
    setError(null);
    try {
      const client = kromaClient();
      const { ceremonyId, options } = await client.passkeyRegisterStart();
      const credential = await createPasskey(options);
      const name = deviceInfo(navigator.userAgent, t('account.unknownDevice')).label;
      await client.passkeyRegisterFinish({ ceremonyId, name, credential });
      await invalidate();
    } catch (e) {
      if (!isCancel(e)) {
        console.error('passkey registration failed', e);
        setError(ceremonyError(e, t('account.passkeyAddFailed')));
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <Panel className="overflow-hidden">
      <div className="flex items-center justify-between gap-4 border-b border-border px-5.5 py-4">
        <div className="flex min-w-0 items-center gap-3.5">
          <span className="flex size-10 flex-none items-center justify-center rounded-[11px] bg-accent-soft text-accent">
            <IconShieldLock size={20} stroke={1.7} />
          </span>
          <div className="min-w-0">
            <div className="font-display text-[15px] font-bold text-text">
              {t('account.passkeys')}
            </div>
            <div className="mt-0.5 text-[12.5px] text-muted">{t('account.passkeysDesc')}</div>
          </div>
        </div>
        {supported ? (
          <Button
            size="sm"
            icon={<IconPlus size={15} />}
            onClick={add}
            disabled={busy}
            className="flex-none"
          >
            {busy ? t('common.saving') : t('account.passkeyAdd')}
          </Button>
        ) : null}
      </div>

      {error ? (
        <div className="px-5.5 py-3 text-[12.5px] font-semibold text-danger">{error}</div>
      ) : null}

      <PasskeysBody supported={supported} keys={keys} onChanged={() => void invalidate()} />
    </Panel>
  );
}

/** The list body: HTTPS notice, empty note, or one row per registered key. */
function PasskeysBody({
  supported,
  keys,
  onChanged,
}: Readonly<{ supported: boolean; keys: PasskeyInfo[] | undefined; onChanged: () => void }>) {
  const t = useT();
  if (!supported)
    return (
      <div className="px-5.5 py-5 text-[13px] text-muted">{t('account.passkeysInsecure')}</div>
    );
  if (!keys || keys.length === 0)
    return <div className="px-5.5 py-5 text-[13px] text-muted">{t('account.passkeysEmpty')}</div>;
  return (
    <div className="divide-y divide-border/70">
      {keys.map((k) => (
        <PasskeyRow key={k.id} passkey={k} onRemoved={onChanged} />
      ))}
    </div>
  );
}
