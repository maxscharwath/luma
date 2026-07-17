import { type AdminUser, type Invite, PERMISSIONS, type Permission } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconMail } from '@tabler/icons-react';
import { useState } from 'react';
import { useAsyncAction } from '#web/features/admin/shell';
import { Field, Modal, ModalActions } from '#web/features/admin/ui';
import { useAuth } from '#web/shared/lib/auth';

export function PendingInvite({ inv, onChange }: Readonly<{ inv: Invite; onChange: () => void }>) {
  const t = useT();
  const { client } = useAuth();
  const [copied, setCopied] = useState(false);
  async function resend() {
    const origin = typeof window !== 'undefined' ? window.location.origin : '';
    try {
      await navigator.clipboard.writeText(`${origin}/join?invite=${inv.token}`);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard blocked */
    }
  }
  return (
    <div className="flex items-center justify-between gap-4 rounded-2xl border border-border bg-surface-1 px-5 py-3.75">
      <div className="flex min-w-0 items-center gap-3.5">
        <span className="flex h-10.5 w-10.5 shrink-0 items-center justify-center rounded-full border border-dashed border-text/25">
          <IconMail size={18} stroke={1.8} color="rgba(244,243,240,.5)" />
        </span>
        <div className="min-w-0">
          <div className="truncate text-[14.5px] font-bold">
            {inv.permissions.join(', ') || t('admin.permPlayback')}
          </div>
          <div className="text-[12.5px] font-medium text-text/45">
            {t('admin.expiresOn', {
              date: new Date(inv.expiresAt * 1000).toLocaleDateString('fr-FR'),
            })}
          </div>
        </div>
      </div>
      <div className="flex shrink-0 gap-2.5">
        <button
          type="button"
          onClick={() => void resend()}
          className="rounded-[9px] border border-border-strong bg-surface-2 px-3.5 py-2 text-[13px] font-semibold text-text/78"
        >
          {copied ? t('common.linkCopied') : t('admin.resend')}
        </button>
        <button
          type="button"
          onClick={() => void client.revokeInvite(inv.token).then(onChange)}
          className="rounded-[9px] border border-[#E8536A]/25 bg-[#E8536A]/10 px-3.5 py-2 text-[13px] font-semibold text-[#E8536A]"
        >
          {t('common.cancel')}
        </button>
      </div>
    </div>
  );
}

function PermPicker({
  selected,
  toggle,
}: Readonly<{
  selected: Set<Permission>;
  toggle: (p: Permission) => void;
}>) {
  const t = useT();
  return (
    <div className="flex flex-col gap-2">
      {PERMISSIONS.map((p) => (
        <label
          key={p.key}
          className="flex cursor-pointer items-center gap-3 rounded-xl px-3 py-2.5 hover:bg-white/3"
        >
          <input
            type="checkbox"
            checked={selected.has(p.key)}
            onChange={() => toggle(p.key)}
            className="h-4 w-4 accent-(--kroma-accent)"
          />
          <span className="min-w-0">
            <span className="block text-[14px] font-semibold">{t(p.labelKey)}</span>
            <span className="block text-[12px] text-dim">{t(p.hintKey)}</span>
          </span>
        </label>
      ))}
    </div>
  );
}

export function EditUserModal({
  user,
  onClose,
  onSaved,
}: Readonly<{
  user: AdminUser;
  onClose: () => void;
  onSaved: () => void;
}>) {
  const t = useT();
  const { client, user: me } = useAuth();
  const [name, setName] = useState(user.username);
  const [perms, setPerms] = useState<Set<Permission>>(new Set(user.permissions));
  const { busy, error, run } = useAsyncAction();
  const isSelf = me?.id === user.id;

  function toggle(p: Permission) {
    setPerms((s) => {
      const next = new Set(s);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });
  }

  const save = () =>
    run(
      async () => {
        await client.updateUser(user.id, { permissions: [...perms], username: name.trim() });
        onSaved();
      },
      () => t('admin.updateFailed'),
    );

  const remove = () => {
    if (!confirm(t('admin.confirmDeleteUser', { name: user.username }))) return;
    run(
      async () => {
        await client.deleteUser(user.id);
        onSaved();
      },
      () => t('admin.deleteFailed'),
    );
  };

  return (
    <Modal title={t('admin.editUser', { name: user.username })} onClose={onClose}>
      <Field label={t('admin.name')}>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="w-full rounded-lg border border-border-strong bg-surface-2 px-3 py-2.5 text-[14px]"
        />
      </Field>
      <div className="mb-2 text-[12px] font-bold uppercase tracking-[.12em] text-dim">
        {t('admin.permissions')}
      </div>
      <PermPicker selected={perms} toggle={toggle} />
      {error ? <p className="mt-3 text-[13px] text-danger">{error}</p> : null}
      <ModalActions
        onCancel={onClose}
        cancelLabel={t('common.cancel')}
        onConfirm={() => void save()}
        confirmLabel={busy ? t('common.saving') : t('common.save')}
        busy={busy}
        destructive={{
          label: t('admin.deleteAccount'),
          onClick: () => void remove(),
          disabled: isSelf,
          title: isSelf ? t('admin.cantDeleteYourself') : undefined,
        }}
      />
    </Modal>
  );
}

export function InviteModal({
  onClose,
  onCreated,
}: Readonly<{ onClose: () => void; onCreated: () => void }>) {
  const t = useT();
  const { client } = useAuth();
  const [perms, setPerms] = useState<Set<Permission>>(new Set<Permission>(['playback']));
  const [link, setLink] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const { busy, run } = useAsyncAction();

  function toggle(p: Permission) {
    setPerms((s) => {
      const next = new Set(s);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });
  }

  const create = () =>
    run(async () => {
      const res = await client.createInvite({ permissions: [...perms] });
      const origin = typeof window !== 'undefined' ? window.location.origin : '';
      setLink(res.url ?? `${origin}/join?invite=${res.token}`);
      onCreated();
    });

  return (
    <Modal title={t('nav.inviteUser')} onClose={onClose}>
      <p className="mb-4 text-[13px] text-dim">{t('admin.inviteIntro')}</p>
      <PermPicker selected={perms} toggle={toggle} />
      {link ? (
        <div className="mt-4 rounded-xl border border-accent/40 bg-accent-soft p-4">
          <div className="mb-2 text-[12px] font-bold uppercase tracking-[.12em] text-accent">
            {t('admin.inviteLink')}
          </div>
          <div className="flex items-center gap-2">
            <input
              readOnly
              value={link}
              onFocus={(e) => e.currentTarget.select()}
              className="min-w-0 flex-1 rounded-lg border border-border-strong bg-surface-2 px-3 py-2.5 text-[13px]"
            />
            <button
              type="button"
              onClick={() => {
                navigator.clipboard.writeText(link).then(
                  () => {
                    setCopied(true);
                    setTimeout(() => setCopied(false), 1500);
                  },
                  () => undefined,
                );
              }}
              className="shrink-0 rounded-lg bg-white/10 px-3.5 py-2.5 text-[13px] font-semibold"
            >
              {copied ? t('common.copied') : t('common.copy')}
            </button>
          </div>
        </div>
      ) : (
        <ModalActions
          onCancel={onClose}
          cancelLabel={t('common.cancel')}
          onConfirm={() => void create()}
          confirmLabel={busy ? t('common.creating') : t('admin.createLink')}
          busy={busy}
          disabled={perms.size === 0}
        />
      )}
    </Modal>
  );
}
