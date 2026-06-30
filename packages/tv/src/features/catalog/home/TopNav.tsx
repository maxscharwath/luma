import { useT } from '@luma/ui';
import { IconSearch } from '@tabler/icons-react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';
import { useNav } from '#tv/app/router';
import { LumaMark, ProfileAvatar, useClock } from '#tv/shared/ui';

export type NavKey = 'home' | 'films' | 'series' | 'mylist' | 'search';

/** The shared 10-foot top bar brand mark, a centred nav pill (Accueil / Films /
 * Séries / Ma liste / Rechercher), the clock and the account avatar (opens the
 * profile menu). Used by Home and the catalogue Grid so the chrome persists. */
export function TvTopNav({ active }: Readonly<{ active: NavKey }>) {
  const nav = useNav();
  const t = useT();
  const clock = useClock();
  const { user } = useAuth();
  const { client } = useConnection();

  const items: { key: NavKey; label: string; search?: boolean; go: () => void }[] = [
    { key: 'home', label: t('nav.home'), go: () => nav.home() },
    { key: 'films', label: t('nav.films'), go: () => nav.reset('grid', { kind: 'films' }) },
    { key: 'series', label: t('nav.series'), go: () => nav.reset('grid', { kind: 'series' }) },
    { key: 'mylist', label: t('nav.myList'), go: () => nav.reset('grid', { kind: 'mylist' }) },
    { key: 'search', label: t('nav.search'), search: true, go: () => nav.reset('search') },
  ];

  return (
    <div className="absolute inset-x-0 top-0 z-10 px-16 py-8">
      {/* Top scrim so the logo / clock / avatar stay readable over bright hero
          art (a sky, a snowy shot…) the hero veil only darkens left + bottom. */}
      <div className="pointer-events-none absolute inset-x-0 top-0 h-36 bg-[linear-gradient(180deg,rgba(10,10,12,0.72),rgba(10,10,12,0.25)_45%,transparent)]" />
      <div className="relative flex items-center justify-between">
        <LumaMark size={28} />
      <nav className="flex items-center gap-1 rounded-full border border-border bg-[rgba(10,10,12,0.5)] p-1.5 backdrop-blur-[10px]">
        {items.map((n) => {
          const on = n.key === active;
          return (
            <button
              key={n.key}
              data-focus=""
              type="button"
              aria-current={on}
              onClick={n.go}
              className={`flex cursor-pointer items-center gap-1.75 rounded-full border-none px-5 py-2.25 font-sans text-[16px] font-semibold outline-none transition-transform focus:scale-[1.04] ${
                on
                  ? 'bg-[rgba(242,180,66,0.16)] text-accent'
                  : 'bg-transparent text-muted focus:text-accent'
              }`}
            >
              {n.search ? <IconSearch size={15} stroke={2} /> : null}
              {n.label}
            </button>
          );
        })}
      </nav>
        <div className="flex items-center gap-4.5">
          <span className="font-sans text-[17px] font-semibold text-text tabular-nums [text-shadow:0_1px_4px_rgba(0,0,0,0.6)]">
            {clock}
          </span>
          {user ? (
            <button
              data-focus=""
              type="button"
              onClick={() => nav.go('profileMenu')}
              title={user.username}
              className="cursor-pointer rounded-[11px] border-none bg-transparent p-0 outline-none transition-transform focus:scale-[1.08]"
            >
              <ProfileAvatar
                name={user.username}
                seed={user.id}
                size={44}
                radius={11}
                src={client?.resolveArt(user.avatarUrl)}
              />
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
