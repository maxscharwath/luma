import type { CastMember } from '@luma/core';
import { useLocale, useT } from '@luma/ui';
import { IconClock, IconPlus, IconVolume, IconVolumeOff } from '@tabler/icons-react';
import { useState } from 'react';
import { useClient, useNav } from '#tv/app/router';
import { AVATAR_GRADS, initials } from '#tv/shared/ui';

/** Wall-clock time `runtimeMs` from now, in the active locale French 24-hour
 * "21h32", else a localised 12/24-hour time. Empty when the runtime is unknown. */
export function endsAtClock(runtimeMs?: number | null, locale?: string): string {
  if (!runtimeMs || runtimeMs <= 0) return '';
  const d = new Date(Date.now() + runtimeMs);
  if (locale === 'en') {
    return d.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
  }
  return `${d.getHours()}h${d.getMinutes().toString().padStart(2, '0')}`;
}

/** "Se termine à 21h32 si vous lancez maintenant" only when a runtime is known. */
export function EndsAtHint({ runtimeMs }: Readonly<{ runtimeMs?: number | null }>) {
  const t = useT();
  const locale = useLocale();
  const at = endsAtClock(runtimeMs, locale);
  if (!at) return null;
  return (
    <div className="mt-3 flex items-center gap-2.25 font-sans text-[15px] font-semibold text-[rgba(244,243,240,0.55)]">
      <IconClock size={16} className="text-accent" stroke={1.8} />
      {t('content.endsAt', { time: at })}
    </div>
  );
}

/** "Distribution" top-billed cast. Shows the real TMDB headshot when present,
 * else a per-position gradient with initials (varied by index so neighbours never
 * collide). Each face is focusable and opens that person's titles. */
export function CastRow({ cast }: Readonly<{ cast?: CastMember[] | null }>) {
  const t = useT();
  const client = useClient();
  const nav = useNav();
  if (!cast || cast.length === 0) return null;
  return (
    <div className="mt-8">
      <div className="mb-4 font-sans text-[15px] font-bold uppercase tracking-[0.04em] text-[rgba(244,243,240,0.55)]">
        {t('content.cast')}
      </div>
      <div className="scrollbar-none flex gap-6 overflow-x-auto px-1.5 py-4.5">
        {cast.slice(0, 16).map((p, i) => (
          <button
            key={`${p.name}-${p.character ?? ''}`}
            data-focus=""
            type="button"
            onClick={() => nav.go('person', { name: p.name })}
            aria-label={t('person.viewWorks', { name: p.name })}
            className="cast-face w-30 flex-none cursor-pointer bg-transparent p-0 text-center"
          >
            <CastAvatar
              photo={client.resolveArt(p.profileUrl)}
              name={p.name}
              grad={CAST_GRADS[i % CAST_GRADS.length] as string}
            />
            <div className="cast-face__name truncate font-sans text-[16px] font-semibold text-text transition-colors">
              {p.name}
            </div>
            {p.character ? (
              <div className="truncate font-sans text-[14px] font-medium text-dim">
                {p.character}
              </div>
            ) : null}
          </button>
        ))}
      </div>
    </div>
  );
}

/** One cast headshot: the photo (over its gradient placeholder, which shows while
 * it loads or if it fails) or initials. */
function CastAvatar({
  photo,
  name,
  grad,
}: Readonly<{ photo: string | null; name: string; grad: string }>) {
  const [failed, setFailed] = useState(false);
  const showImg = Boolean(photo) && !failed;
  return (
    <div
      className="cast-face__avatar relative mb-3 flex h-30 w-30 items-center justify-center overflow-hidden rounded-full font-display text-[40px] font-bold text-[rgba(255,255,255,0.9)] shadow-card"
      style={{ background: grad }}
    >
      <div className="absolute inset-0 bg-[radial-gradient(70%_60%_at_50%_22%,rgba(255,255,255,0.2),transparent_60%)]" />
      {showImg ? (
        <img
          src={photo ?? undefined}
          alt=""
          onError={() => setFailed(true)}
          className="absolute inset-0 h-full w-full object-cover"
        />
      ) : (
        initials(name)
      )}
    </div>
  );
}

/** Check mark glyph (used by the my-list and watched toggles). */
function CheckGlyph() {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

/** Shared pill classes for the detail action toggles (active = amber). */
const detailToggle = (active: boolean) =>
  `inline-flex cursor-pointer items-center gap-2.75 rounded-[13px] border px-7.5 py-4 font-sans text-[19px] font-semibold transition-transform focus:scale-[1.04] ${
    active
      ? 'border-[rgba(242,180,66,0.45)] bg-accent-soft text-accent'
      : 'border-[rgba(255,255,255,0.2)] bg-[rgba(255,255,255,0.12)] text-text'
  }`;

/** My-list toggle (visual; not yet persisted server-side). */
export function ListButton({
  inList,
  onToggle,
}: Readonly<{ inList: boolean; onToggle: () => void }>) {
  const t = useT();
  return (
    <button data-focus="" type="button" onClick={onToggle} className={detailToggle(inList)}>
      {inList ? <CheckGlyph /> : <IconPlus size={20} stroke={2} />}
      {inList ? t('content.inList') : t('content.addToList')}
    </button>
  );
}

/** Watched toggle marks a title seen / unseen (persisted via the watched API). */
export function WatchedButton({
  watched,
  onToggle,
}: Readonly<{ watched: boolean; onToggle: () => void }>) {
  const t = useT();
  return (
    <button
      data-focus=""
      type="button"
      onClick={onToggle}
      aria-pressed={watched}
      aria-label={watched ? t('content.markUnwatched') : t('content.markWatched')}
      className={detailToggle(watched)}
    >
      <CheckGlyph />
      {watched ? t('content.watched') : t('content.markWatched')}
    </button>
  );
}

/** Round mute toggle for the show's theme song (remote-focusable). */
export function ThemeButton({
  muted,
  onToggle,
}: Readonly<{ muted: boolean; onToggle: () => void }>) {
  const t = useT();
  const label = muted ? t('content.unmuteTheme') : t('content.muteTheme');
  return (
    <button
      data-focus=""
      type="button"
      onClick={onToggle}
      aria-label={label}
      title={label}
      className="inline-flex h-15 w-15 cursor-pointer items-center justify-center rounded-full border border-[rgba(255,255,255,0.2)] bg-[rgba(255,255,255,0.12)] text-text transition-transform focus:scale-[1.04]"
    >
      {muted ? <IconVolumeOff size={24} stroke={2} /> : <IconVolume size={24} stroke={2} />}
    </button>
  );
}

// Cast-circle gradients, cycled by position so adjacent faces never share a colour.
const CAST_GRADS = [...AVATAR_GRADS, 'linear-gradient(135deg,#FBBF24,#F97316)'];
