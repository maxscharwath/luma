// Security card: self-service password change (current + new + confirm). There
// is no email-based reset flow the server has no mail service so this is how an
// account rotates its own password. On success the fields clear.

import { useT } from '@luma/ui';
import { useState } from 'react';
import { Card, Field, StatusText, useSave } from '#web/features/accounts/account/ui';
import { useAuth } from '#web/shared/lib/auth';
import { Button } from '#web/shared/ui';

export function SecurityCard() {
  const t = useT();
  const { client } = useAuth();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [confirm, setConfirm] = useState('');
  const [mismatch, setMismatch] = useState(false);
  const save = useSave();

  const valid = current.length > 0 && next.length >= 4 && confirm.length > 0;

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!valid) return;
    if (next !== confirm) {
      setMismatch(true);
      return;
    }
    setMismatch(false);
    save.run(async () => {
      await client.changePassword(current, next);
      setCurrent('');
      setNext('');
      setConfirm('');
    }, t('account.saveFailed'));
  };

  return (
    <Card title={t('account.password')} desc={t('account.passwordSub')}>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field
          label={t('account.currentPassword')}
          type="password"
          autoComplete="current-password"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
        />
        <Field
          label={t('account.newPassword')}
          type="password"
          autoComplete="new-password"
          hint={t('auth.passwordHint')}
          value={next}
          onChange={(e) => setNext(e.target.value)}
        />
        <Field
          label={t('account.confirmPassword')}
          type="password"
          autoComplete="new-password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
        />
        <div className="flex items-center gap-3">
          <Button type="submit" size="sm" disabled={!valid || save.status === 'saving'}>
            {save.status === 'saving' ? t('common.saving') : t('account.updatePassword')}
          </Button>
          {mismatch ? (
            <span className="text-[13px] font-medium text-danger">
              {t('account.passwordMismatch')}
            </span>
          ) : (
            <StatusText status={save.status} error={save.error} />
          )}
        </div>
      </form>
    </Card>
  );
}
