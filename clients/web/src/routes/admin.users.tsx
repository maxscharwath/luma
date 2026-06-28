import type { AdminUser, Invite, MessageKey, Permission } from '@luma/core';
import { useT } from '@luma/ui';
import { IconDots, IconMail } from '@tabler/icons-react';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { Denied, HeaderAction, PageHeader, useCap, usePoll } from '#web/components/admin/shell';
import { Avatar, C, Card, Modal, Section, StatCard } from '#web/components/admin/ui';
import { relativeSeen } from '#web/lib/adminFormat';
import { useAuth } from '#web/lib/auth';

export const Route = createFileRoute('/admin/users')({
  component: UsersPage,
});

const PERMS: { key: Permission; labelKey: MessageKey; hintKey: MessageKey }[] = [
  { key: 'playback', labelKey: 'admin.permPlayback', hintKey: 'admin.permPlaybackHint' },
  { key: 'library.manage', labelKey: 'admin.permLibrary', hintKey: 'admin.permLibraryHint' },
  { key: 'users.manage', labelKey: 'admin.permUsers', hintKey: 'admin.permUsersHint' },
  { key: 'settings.manage', labelKey: 'admin.permSettings', hintKey: 'admin.permSettingsHint' },
];

// Roles arrive already localized from the server (Accept-Language synced), so we
// match both locale spellings to keep the accent color right regardless of UI lang.
function roleStyle(role: string): { c: string; bg: string } {
  if (role === 'Propriétaire' || role === 'Owner')
    return { c: C.accent, bg: 'rgba(242,180,66,.16)' };
  if (role === 'Restreint' || role === 'Restricted')
    return { c: '#86A8FF', bg: 'rgba(134,168,255,.14)' };
  return { c: C.green, bg: 'rgba(70,208,141,.14)' };
}

function UsersPage() {
  if (!useCap('users.manage')) return <Denied />;
  return <UsersPageInner />;
}

function UsersPageInner() {
  const t = useT();
  const { client } = useAuth();
  const { data, reload } = usePoll(() => client.adminUsers(), 8000, [client]);
  const { data: invitesData, reload: reloadInvites } = usePoll(() => client.invites(), 15000, [
    client,
  ]);
  const [editing, setEditing] = useState<AdminUser | null>(null);
  const [inviting, setInviting] = useState(false);

  const users = data?.users ?? [];
  const libraryCount = data?.libraryCount ?? 0;
  const invites = invitesData ?? [];
  const online = users.filter((u) => u.online).length;

  return (
    <>
      <PageHeader
        title={t('admin.usersTitle')}
        subtitle={t('admin.usersSub')}
        action={<HeaderAction label={t('nav.inviteUser')} onClick={() => setInviting(true)} />}
      />

      <div className="mt-6 grid grid-cols-3 gap-4">
        <StatCard
          label={t('admin.statUsers')}
          value={users.length}
          unit={t('admin.statAccounts')}
        />
        <StatCard
          label={t('admin.statOnline')}
          value={online}
          unit={t('admin.statNow')}
          color={C.green}
        />
        <StatCard
          label={t('admin.statInvites')}
          value={invites.length}
          unit={t('admin.statPending')}
          color={C.accent}
        />
      </div>

      <Section title={t('admin.membersSharing')}>
        <Card className="overflow-hidden">
          <div className="grid grid-cols-[2.4fr_1fr_1.3fr_1.2fr_44px] gap-4 border-b border-border bg-surface-2 px-5.5 py-3.5 text-[10px] font-bold uppercase tracking-[.12em] text-text/40">
            <span>{t('admin.colUser')}</span>
            <span>{t('admin.colRole')}</span>
            <span>{t('admin.colAccess')}</span>
            <span>{t('admin.colLastActivity')}</span>
            <span />
          </div>
          {users.map((u) => {
            const rs = roleStyle(u.role);
            const access =
              u.role === 'Propriétaire' || u.role === 'Owner'
                ? t('admin.allLibraries')
                : t('admin.libraryCount', { count: libraryCount });
            return (
              <div
                key={u.id}
                className="grid grid-cols-[2.4fr_1fr_1.3fr_1.2fr_44px] items-center gap-4 border-b border-white/4 px-5.5 py-3.75"
              >
                <div className="flex min-w-0 items-center gap-3.5">
                  <Avatar name={u.username} avatarUrl={u.avatarUrl} size={42} />
                  <div className="min-w-0">
                    <div className="truncate text-[14.5px] font-bold">{u.username}</div>
                    <div className="truncate text-[12.5px] font-medium text-text/45">{u.email}</div>
                  </div>
                </div>
                <div>
                  <span
                    className="inline-flex rounded-full px-2.75 py-1.25 text-[11.5px] font-bold"
                    style={{ color: rs.c, background: rs.bg }}
                  >
                    {u.role}
                  </span>
                </div>
                <div className="text-[13px] font-semibold text-text/72">{access}</div>
                <div className="inline-flex items-center gap-2 text-[13px] font-semibold text-text/60">
                  <span
                    className="h-1.75 w-1.75 rounded-full"
                    style={{ background: u.online ? C.green : 'rgba(244,243,240,.3)' }}
                  />
                  {u.online ? t('admin.online') : relativeSeen(u.lastSeen)}
                </div>
                <button
                  type="button"
                  onClick={() => setEditing(u)}
                  className="flex justify-end text-text/50 hover:text-text"
                  aria-label={t('admin.editUserAction')}
                >
                  <IconDots size={18} stroke={2} />
                </button>
              </div>
            );
          })}
        </Card>
      </Section>

      {invites.length > 0 ? (
        <Section title={t('admin.pendingInvites')}>
          <div className="flex flex-col gap-3">
            {invites.map((inv) => (
              <PendingInvite key={inv.token} inv={inv} onChange={reloadInvites} />
            ))}
          </div>
        </Section>
      ) : null}

      {editing ? (
        <EditUserModal
          user={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            reload();
          }}
        />
      ) : null}
      {inviting ? (
        <InviteModal
          onClose={() => setInviting(false)}
          onCreated={() => {
            reloadInvites();
          }}
        />
      ) : null}
    </>
  );
}

function PendingInvite({ inv, onChange }: Readonly<{ inv: Invite; onChange: () => void }>) {
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
      {PERMS.map((p) => (
        <label
          key={p.key}
          className="flex cursor-pointer items-center gap-3 rounded-xl px-3 py-2.5 hover:bg-white/3"
        >
          <input
            type="checkbox"
            checked={selected.has(p.key)}
            onChange={() => toggle(p.key)}
            className="h-4 w-4 accent-(--luma-accent)"
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

function EditUserModal({
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
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const isSelf = me?.id === user.id;

  function toggle(p: Permission) {
    setPerms((s) => {
      const next = new Set(s);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });
  }

  async function save() {
    setBusy(true);
    setErr(null);
    try {
      await client.updateUser(user.id, { permissions: [...perms], username: name.trim() });
      onSaved();
    } catch {
      setErr(t('admin.updateFailed'));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!confirm(t('admin.confirmDeleteUser', { name: user.username }))) return;
    setBusy(true);
    setErr(null);
    try {
      await client.deleteUser(user.id);
      onSaved();
    } catch {
      setErr(t('admin.deleteFailed'));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title={t('admin.editUser', { name: user.username })} onClose={onClose}>
      <label className="mb-1.5 block text-[12px] font-bold uppercase tracking-[.12em] text-dim">
        {t('admin.name')}
      </label>
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        className="mb-4 w-full rounded-lg border border-border-strong bg-surface-2 px-3 py-2.5 text-[14px]"
      />
      <div className="mb-2 text-[12px] font-bold uppercase tracking-[.12em] text-dim">
        {t('admin.permissions')}
      </div>
      <PermPicker selected={perms} toggle={toggle} />
      {err ? <p className="mt-3 text-[13px] text-danger">{err}</p> : null}
      <div className="mt-5 flex items-center justify-between gap-3">
        <button
          type="button"
          onClick={() => void remove()}
          disabled={busy || isSelf}
          className="text-[13px] font-semibold text-[#E8536A] disabled:opacity-40"
          title={isSelf ? t('admin.cantDeleteYourself') : undefined}
        >
          {t('admin.deleteAccount')}
        </button>
        <div className="flex gap-2.5">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-4 py-2.5 text-[14px] font-semibold text-muted"
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            onClick={() => void save()}
            disabled={busy}
            className="rounded-md bg-accent px-5 py-2.5 text-[14px] font-bold text-accent-ink disabled:opacity-50"
          >
            {busy ? t('common.saving') : t('common.save')}
          </button>
        </div>
      </div>
    </Modal>
  );
}

function InviteModal({
  onClose,
  onCreated,
}: Readonly<{ onClose: () => void; onCreated: () => void }>) {
  const t = useT();
  const { client } = useAuth();
  const [perms, setPerms] = useState<Set<Permission>>(new Set<Permission>(['playback']));
  const [link, setLink] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  function toggle(p: Permission) {
    setPerms((s) => {
      const next = new Set(s);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });
  }

  async function create() {
    setBusy(true);
    try {
      const res = await client.createInvite({ permissions: [...perms] });
      const origin = typeof window !== 'undefined' ? window.location.origin : '';
      setLink(res.url ?? `${origin}/join?invite=${res.token}`);
      onCreated();
    } finally {
      setBusy(false);
    }
  }

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
        <div className="mt-5 flex justify-end gap-2.5">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-4 py-2.5 text-[14px] font-semibold text-muted"
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            onClick={() => void create()}
            disabled={busy || perms.size === 0}
            className="rounded-md bg-accent px-5 py-2.5 text-[14px] font-bold text-accent-ink disabled:opacity-50"
          >
            {busy ? t('common.creating') : t('admin.createLink')}
          </button>
        </div>
      )}
    </Modal>
  );
}
