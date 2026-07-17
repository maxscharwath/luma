// Security section: self-service password change (current + new + confirm) with
// a live strength meter. There is no email-based reset flow the server has no
// mail service so this is how an account rotates its own password. On success
// the fields clear.

import { useT } from '@kroma/ui';
import { IconDeviceFloppy } from '@tabler/icons-react';
import { useState } from 'react';
import {
  LabeledInput,
  Panel,
  passwordStrength,
  StatusText,
  useSave,
} from '#web/features/accounts/account/ui';
import { useAuth } from '#web/shared/lib/auth';
import { Button } from '#web/shared/ui';

export function SecurityCard() {
  const t = useT();
  const { client } = useAuth();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [confirm, setConfirm] = useState('');
  const save = useSave();

  const strength = passwordStrength(next);
  const mismatch = confirm.length > 0 && next !== confirm;
  const valid = current.length > 0 && next.length >= 4 && confirm.length > 0 && !mismatch;

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!valid) return;
    save.run(async () => {
      await client.changePassword(current, next);
      setCurrent('');
      setNext('');
      setConfirm('');
    }, t('account.saveFailed'));
  };

  return (
    <Panel className="p-5.5">
      <div className="mb-1 font-display text-[15px] font-bold text-text">
        {t('account.updatePassword')}
      </div>
      <div className="mb-4.5 text-[12.5px] text-muted">{t('auth.passwordHint')}</div>

      <form onSubmit={submit} className="grid grid-cols-1 gap-4.5 sm:grid-cols-2">
        <LabeledInput
          className="sm:col-span-2 sm:max-w-[calc(50%-0.5625rem)]"
          label={t('account.currentPassword')}
          type="password"
          autoComplete="current-password"
          placeholder="••••••••"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
        />

        <div className="flex flex-col gap-2.5">
          <LabeledInput
            label={t('account.newPassword')}
            type="password"
            autoComplete="new-password"
            placeholder="••••••••"
            value={next}
            onChange={(e) => setNext(e.target.value)}
          />
          <div className="flex items-center gap-2.5">
            <div className="h-[5px] flex-1 overflow-hidden rounded-full bg-white/10">
              <div
                className="h-full rounded-full transition-[width,background-color] duration-200"
                style={{ width: strength.width, background: strength.color }}
              />
            </div>
            {strength.labelKey ? (
              <span
                className="min-w-[54px] text-right text-[11px] font-bold"
                style={{ color: strength.color }}
              >
                {t(strength.labelKey)}
              </span>
            ) : null}
          </div>
        </div>

        <div className="flex flex-col gap-2.5">
          <LabeledInput
            label={t('account.confirmPassword')}
            type="password"
            autoComplete="new-password"
            placeholder="••••••••"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
          />
          {mismatch ? (
            <span className="text-[11.5px] font-semibold text-danger">
              {t('account.passwordMismatch')}
            </span>
          ) : null}
        </div>

        <div className="flex items-center gap-3 sm:col-span-2">
          <Button
            type="submit"
            size="sm"
            icon={<IconDeviceFloppy size={16} />}
            disabled={!valid || save.status === 'saving'}
          >
            {save.status === 'saving' ? t('common.saving') : t('account.updatePassword')}
          </Button>
          <StatusText status={save.status} error={save.error} />
        </div>
      </form>
    </Panel>
  );
}
