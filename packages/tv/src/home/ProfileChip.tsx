import { useT } from '@luma/ui';
import { useAuth } from '#tv/auth';
import { useClient } from '#tv/router';

// Account avatar palette (shared look with the web profiles / LUMA.dc.html).
const AVATAR_GRADS = [
  'linear-gradient(135deg,#F4B642,#E8743B)',
  'linear-gradient(135deg,#3BC9DB,#3B82F6)',
  'linear-gradient(135deg,#A855F7,#6366F1)',
  'linear-gradient(135deg,#F472B6,#EC4899)',
  'linear-gradient(135deg,#34D399,#10B981)',
];

function gradFor(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i += 1) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return AVATAR_GRADS[h % AVATAR_GRADS.length] as string;
}

function userInitials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase();
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase();
}

/** Header account chip — focusable; OK returns to the profile picker WITHOUT
 * signing out, so this device stays remembered and switching back is password-
 * free (same as the web). Reads the user + switchProfile from the auth context. */
export function ProfileChip() {
  const client = useClient();
  const t = useT();
  const { user, switchProfile } = useAuth();
  if (!user) return null;
  const avatar = client.resolveArt(user.avatarUrl);
  return (
    <div
      data-focus=""
      tabIndex={0}
      role="button"
      title={t('nav.changeProfile')}
      onClick={switchProfile}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') switchProfile();
      }}
      className="flex cursor-pointer items-center gap-2.5 rounded-full border border-border bg-[rgba(10,10,12,0.42)] py-1.5 pl-1.5 pr-4 outline-none backdrop-blur-[10px] transition-transform focus:scale-[1.06]"
    >
      {avatar ? (
        <img src={avatar} alt="" className="h-9 w-9 rounded-full object-cover" />
      ) : (
        <div
          className="flex h-9 w-9 items-center justify-center rounded-full font-display text-[15px] font-bold text-white/90"
          style={{ background: gradFor(user.id) }}
        >
          {userInitials(user.username)}
        </div>
      )}
      <span className="font-sans text-[15px] font-semibold text-text">{user.username}</span>
    </div>
  );
}
