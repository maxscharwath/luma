// PIN card: set / change / remove the account's 4-digit profile-lock PIN. The
// PIN gates switching into this profile on a shared device (mainly the TV app);
// it's not the login credential. Uses the existing `setPin`/`clearPin` endpoints,
// which verify the current PIN when one is already set.

import { useT } from '@kroma/ui';
import { IconLock } from '@tabler/icons-react';
import { useState } from 'react';
import { Panel, StatusText, useSave } from '#web/features/accounts/account/ui';
import { useAuth } from '#web/shared/lib/auth';
import { Button, Otp } from '#web/shared/ui';

/** A labelled masked 4-digit PIN field (shared/ui Otp). */
function PinField({
  label,
  value,
  onChange,
}: Readonly<{ label: string; value: string; onChange: (v: string) => void }>) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-[11px] font-bold uppercase tracking-[0.08em] text-dim">{label}</span>
      <Otp value={value} onChange={onChange} mask ariaLabel={label} />
    </div>
  );
}

export function PinCard() {
  const t = useT();
  const { user, client, updateUser } = useAuth();
  const [current, setCurrent] = useState('');
  const [pin, setPin] = useState('');
  const [confirm, setConfirm] = useState('');
  const [mismatch, setMismatch] = useState(false);
  const save = useSave();
  const remove = useSave();

  if (!user) return null;
  const hasPin = user.hasPin;
  const submitLabel = hasPin ? t('account.changePin') : t('account.setPin');

  const reset = () => {
    setCurrent('');
    setPin('');
    setConfirm('');
  };

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (pin.length !== 4 || (hasPin && current.length !== 4)) return;
    if (pin !== confirm) {
      setMismatch(true);
      return;
    }
    setMismatch(false);
    save.run(async () => {
      const { user: u } = await client.setPin(pin, hasPin ? current : undefined);
      updateUser({ hasPin: u.hasPin });
      reset();
    }, t('account.saveFailed'));
  };

  const removePin = () => {
    if (current.length !== 4) return;
    remove.run(async () => {
      const { user: u } = await client.clearPin(current);
      updateUser({ hasPin: u.hasPin });
      reset();
    }, t('account.saveFailed'));
  };

  return (
    <Panel className="p-5.5">
      <div className="mb-4 flex items-center gap-3.5">
        <span className="flex size-10 flex-none items-center justify-center rounded-[11px] bg-accent-soft text-accent">
          <IconLock size={20} stroke={1.8} />
        </span>
        <div className="min-w-0">
          <div className="font-display text-[15px] font-bold text-text">{t('account.pin')}</div>
          <div className="mt-0.5 text-[12.5px] text-muted">
            {hasPin ? t('account.pinSubSet') : t('account.pinSub')}
          </div>
        </div>
      </div>

      <form onSubmit={submit} className="flex flex-col gap-4">
        {hasPin ? (
          <PinField label={t('account.currentPin')} value={current} onChange={setCurrent} />
        ) : null}
        <PinField
          label={hasPin ? t('account.newPin') : t('account.pin')}
          value={pin}
          onChange={setPin}
        />
        <PinField label={t('account.confirmPin')} value={confirm} onChange={setConfirm} />
        <div className="flex flex-wrap items-center gap-3">
          <Button
            type="submit"
            size="sm"
            disabled={
              pin.length !== 4 || (hasPin && current.length !== 4) || save.status === 'saving'
            }
          >
            {save.status === 'saving' ? t('common.saving') : submitLabel}
          </Button>
          {hasPin ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={removePin}
              disabled={current.length !== 4 || remove.status === 'saving'}
            >
              {remove.status === 'saving' ? t('common.saving') : t('account.removePin')}
            </Button>
          ) : null}
          {mismatch ? (
            <span className="text-[13px] font-medium text-danger">{t('account.pinMismatch')}</span>
          ) : (
            <StatusText
              status={save.status === 'idle' ? remove.status : save.status}
              error={save.error ?? remove.error}
            />
          )}
        </div>
      </form>
    </Panel>
  );
}
