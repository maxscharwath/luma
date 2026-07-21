import type { AdminUser } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconDots, IconUsers } from '@tabler/icons-react';
import { Denied, HeaderAction, PageHeader, useCap, usePoll } from '#web/features/admin/shell';
import { Avatar, C, Card, Section, StatCard } from '#web/features/admin/ui';
import { EditUserModal, InviteModal, PendingInvite } from '#web/features/admin/users-modals';
import { relativeSeen } from '#web/shared/lib/adminFormat';
import { useAuth } from '#web/shared/lib/auth';
import { EmptyState } from '#web/shared/ui';

// Roles arrive already localized from the server (Accept-Language synced), so we
// match both locale spellings to keep the accent color right regardless of UI lang.
function roleStyle(role: string): { c: string; bg: string } {
  if (role === 'Propriétaire' || role === 'Owner')
    return { c: C.accent, bg: 'rgba(242,180,66,.16)' };
  if (role === 'Restreint' || role === 'Restricted')
    return { c: '#86A8FF', bg: 'rgba(134,168,255,.14)' };
  return { c: C.green, bg: 'rgba(70,208,141,.14)' };
}

export function UsersScreen() {
  if (!useCap('users.manage')) return <Denied />;
  return <UsersPageInner />;
}

function UsersPageInner() {
  const t = useT();
  const { client } = useAuth();
  const { data, reload } = usePoll(['admin', 'users'], () => client.adminUsers(), 8000);
  const { data: invitesData, reload: reloadInvites } = usePoll(
    ['admin', 'invites'],
    () => client.invites(),
    15000,
  );
  const openInvite = async () => {
    if (await InviteModal.call()) reloadInvites();
  };
  const openEdit = async (user: AdminUser) => {
    if (await EditUserModal.call({ user })) reload();
  };

  const users = data?.users ?? [];
  const libraryCount = data?.libraryCount ?? 0;
  const invites = invitesData ?? [];
  const online = users.filter((u) => u.online).length;

  return (
    <>
      <PageHeader
        title={t('admin.usersTitle')}
        subtitle={t('admin.usersSub')}
        action={<HeaderAction label={t('nav.inviteUser')} onClick={() => void openInvite()} />}
      />

      <div className="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-3">
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
          <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-4 border-b border-border bg-surface-2 px-5.5 py-3.5 text-[10px] font-bold uppercase tracking-[.12em] text-text/40 md:grid-cols-[2.4fr_1fr_1.3fr_1.2fr_44px]">
            <span>{t('admin.colUser')}</span>
            <span className="max-md:hidden">{t('admin.colRole')}</span>
            <span className="max-md:hidden">{t('admin.colAccess')}</span>
            <span className="max-md:hidden">{t('admin.colLastActivity')}</span>
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
                className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 border-b border-white/4 px-5.5 py-3.75 md:grid-cols-[2.4fr_1fr_1.3fr_1.2fr_44px]"
              >
                <div className="flex min-w-0 items-center gap-3.5">
                  <Avatar name={u.username} avatarUrl={u.avatarUrl} size={42} />
                  <div className="min-w-0">
                    <div className="truncate text-[14.5px] font-bold">{u.username}</div>
                    <div className="truncate text-[12.5px] font-medium text-text/45">{u.email}</div>
                  </div>
                </div>
                <div className="max-md:hidden">
                  <span
                    className="inline-flex rounded-full px-2.75 py-1.25 text-[11.5px] font-bold"
                    style={{ color: rs.c, background: rs.bg }}
                  >
                    {u.role}
                  </span>
                </div>
                <div className="text-[13px] font-semibold text-text/72 max-md:hidden">{access}</div>
                <div className="inline-flex items-center gap-2 text-[13px] font-semibold text-text/60 max-md:hidden">
                  <span
                    className="h-1.75 w-1.75 rounded-full"
                    style={{ background: u.online ? C.green : 'rgba(244,243,240,.3)' }}
                  />
                  {u.online ? t('admin.online') : relativeSeen(u.lastSeen)}
                </div>
                <button
                  type="button"
                  onClick={() => void openEdit(u)}
                  className="flex justify-end text-text/50 hover:text-text"
                  aria-label={t('admin.editUserAction')}
                >
                  <IconDots size={18} stroke={2} />
                </button>
              </div>
            );
          })}
          {data && users.length === 0 ? (
            <EmptyState
              icon={<IconUsers size={32} stroke={1.5} />}
              title={t('admin.usersEmpty')}
              hint={t('admin.usersEmptyHint')}
              action={
                <HeaderAction label={t('nav.inviteUser')} onClick={() => void openInvite()} />
              }
            />
          ) : null}
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
    </>
  );
}
