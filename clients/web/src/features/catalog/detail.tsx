import {
  type CastMember,
  canDirectPlay,
  channelLabel,
  codecLabel,
  langName,
  type MediaItem,
  posterColors,
  type Translate,
  type VideoTrack,
} from '@luma/core';
import { useT, useThemeAudio } from '@luma/ui';
import {
  IconCheck,
  IconChevronLeft,
  IconPlayerPlayFilled,
  IconPlus,
  IconVolume,
  IconVolumeOff,
} from '@tabler/icons-react';
import { useNavigate } from '@tanstack/react-router';
import { type ReactNode, useEffect, useState } from 'react';
import { HeroBackdrop } from '#web/features/catalog/hero-backdrop';
import { imageUrl } from '#web/shared/lib/api';
import { Avatar, AvatarFallback, AvatarImage, Badge, Button, Poster, Rail } from '#web/shared/ui';

export type QualityTone = '4K' | 'HDR' | 'H.265';

/** Quality pills shown beside the meta line (mirrors the design's `cur.badges`). */
export function qualityBadges(video: VideoTrack | null | undefined): QualityTone[] {
  if (!video) return [];
  const out: QualityTone[] = [];
  if ((video.width ?? 0) >= 3840) out.push('4K');
  if (video.hdr) out.push('HDR');
  if (video.codec === 'hevc') out.push('H.265');
  return out;
}

// `langName` is re-exported so existing importers (AvDrawer, movie route) keep
// their `#web/features/catalog/detail` path; the implementation now lives in @luma/core.
export { langName };

/** "Français · AAC 5.1" language then codec/channels. */
export function audioString(t: Translate, item: Pick<MediaItem, 'audio'>): string {
  const a = item.audio;
  if (!a) return '-';
  const tech = [codecLabel(a.codec), channelLabel(a.channels)].filter(Boolean).join(' ');
  return [langName(t, a.language), tech].filter(Boolean).join(' · ') || '-';
}

/** Distinct subtitle languages, or "Aucun". */
export function subString(t: Translate, item: Pick<MediaItem, 'subtitles'>): string {
  const langs = [...new Set(item.subtitles.map((s) => langName(t, s.language)).filter(Boolean))];
  return langs.length ? langs.join(', ') : t('subtitle.none');
}

function PlayIcon() {
  return <IconPlayerPlayFilled size={18} />;
}

function Field({ label, value }: Readonly<{ label: string; value: string }>) {
  return (
    <div>
      <div className="mb-1.75 text-[11px] font-semibold uppercase tracking-widest text-white/45">
        {label}
      </div>
      <div className="text-[14px] font-medium text-white/85">{value}</div>
    </div>
  );
}

export interface DetailHeroProps {
  /** Identity + artwork for the key-art backdrop and poster. */
  art: { id: string; backdrop: string | null; poster: string };
  /** Amber overline above the title (e.g. genres, or "Série · 2 saisons"). */
  overline: string;
  title: string;
  rating?: number | null;
  /** Terse meta line, e.g. "2024 · 2h08 · Français". */
  meta: string;
  badges: QualityTone[];
  /** Director(s) / creator(s), shown as a "Réalisation" line. */
  directors?: string[];
  tagline?: string | null;
  overview?: string | null;
  /** Primary audio line; omit to hide the audio/subtitle fields (not-owned titles). */
  audio?: string;
  subtitles?: string;
  playLabel?: string;
  /** Replaces the default Play button (e.g. a Request CTA / status chip for a
   * title that isn't in the library). When set, `onPlay` is ignored. */
  primaryAction?: ReactNode;
  onBack: () => void;
  onPlay?: () => void;
  /** Watched state for the title; omit (undefined) to hide the watched toggle. */
  watched?: boolean;
  /** Flip the watched flag. Required for the watched toggle to render. */
  onToggleWatched?: () => void;
  /** Whether the title is in "Ma liste" (drives the + / ✓ button). */
  inList?: boolean;
  /** Flip "Ma liste" membership. Required for the list button to be interactive. */
  onToggleList?: () => void;
  /** Item whose codecs gate direct-play; the warning is computed client-side. */
  playable?: MediaItem | null;
  /** Plex-style theme song to loop under the hero (TV shows only); `null` plays
   * nothing and hides the mute toggle. */
  themeUrl?: string | null;
  /** Optional trailing action in the button row (e.g. an admin "Reprocess"). */
  adminAction?: ReactNode;
}

/** Director/creator names from TMDB crew metadata, for the hero's "Réalisation"
 * line (empty when crew isn't resolved yet). */
export function directorsOf(meta?: { crew?: { name: string; job: string }[] } | null): string[] {
  return (meta?.crew ?? [])
    .filter((c) => c.job === 'Director' || c.job === 'Creator')
    .map((c) => c.name);
}

/** Full-bleed cinematic detail hero shared by the movie and series fiches
 * (matches the web DETAIL section of LUMA.dc.html). */
export function DetailHero({
  art,
  overline,
  title,
  rating,
  meta,
  badges,
  directors,
  tagline,
  overview,
  audio,
  subtitles,
  playLabel,
  primaryAction,
  onBack,
  onPlay,
  watched,
  onToggleWatched,
  inList,
  onToggleList,
  playable,
  themeUrl,
  adminAction,
}: Readonly<DetailHeroProps>) {
  const t = useT();
  const navigate = useNavigate();
  const [c1, c2] = posterColors(art.id);
  const heroBg = art.backdrop ? `url("${art.backdrop}")` : `linear-gradient(135deg, ${c1}, ${c2})`;
  const theme = useThemeAudio(themeUrl);

  // Direct-play depends on the runtime's codecs (navigator/MediaSource), so it
  // must stay client-only computing it during SSR would mismatch on hydration.
  const [unsupported, setUnsupported] = useState<string | null>(null);
  useEffect(() => {
    if (!playable) return setUnsupported(null);
    const v = canDirectPlay(playable);
    setUnsupported(v.canDirectPlay ? null : t(v.messageKey, v.messageVars));
  }, [playable, t]);

  return (
    <div className="relative min-h-[62vh]">
      <HeroBackdrop bg={heroBg} />

      <button
        type="button"
        onClick={onBack}
        aria-label={t('common.back')}
        className="absolute left-8 top-6.5 z-3 flex h-10.5 w-10.5 items-center justify-center rounded-full
          border border-white/12 bg-[rgba(10,10,12,.5)] backdrop-blur-sm transition-colors hover:bg-[rgba(10,10,12,.8)]"
      >
        <IconChevronLeft size={20} stroke={2} color="#fff" />
      </button>

      {theme.active ? (
        <button
          type="button"
          onClick={theme.toggle}
          aria-label={theme.muted ? t('content.unmuteTheme') : t('content.muteTheme')}
          title={theme.muted ? t('content.unmuteTheme') : t('content.muteTheme')}
          className="absolute right-8 top-6.5 z-3 flex h-10.5 w-10.5 items-center justify-center rounded-full
            border border-white/12 bg-[rgba(10,10,12,.5)] backdrop-blur-sm transition-colors hover:bg-[rgba(10,10,12,.8)]"
        >
          {theme.muted ? (
            <IconVolumeOff size={19} stroke={2} color="#fff" />
          ) : (
            <IconVolume size={19} stroke={2} color="#fff" />
          )}
        </button>
      ) : null}

      <div className="relative flex flex-wrap items-end gap-10 px-(--gutter-web) pb-9 pt-22.5">
        <div
          className="relative aspect-2/3 w-60 shrink-0 overflow-hidden rounded-[14px] shadow-hero"
          style={{ background: `linear-gradient(158deg, ${c1}, ${c2})` }}
        >
          <img
            src={art.poster}
            alt=""
            draggable={false}
            className="absolute inset-0 h-full w-full object-cover"
          />
        </div>

        <div className="max-w-170 flex-1 [text-shadow:0_1px_3px_rgba(0,0,0,.5),0_2px_16px_rgba(0,0,0,.55)]">
          <div className="mb-3 text-[12px] font-semibold tracking-[.18em] text-accent">
            {overline}
          </div>
          <h1 className="mb-4 font-display text-[56px] font-bold leading-none tracking-[-.02em] [text-shadow:0_0_2px_rgba(0,0,0,.55),0_2px_8px_rgba(0,0,0,.55),0_8px_30px_rgba(0,0,0,.6)]">
            {title}
          </h1>

          <div className="mb-4.5 flex flex-wrap items-center gap-2.5">
            {rating ? (
              <>
                <span className="text-[14px] font-bold text-accent">{rating.toFixed(1)}★</span>
                <span className="text-white/40">·</span>
              </>
            ) : null}
            <span className="text-[14px] font-medium text-white/72">{meta}</span>
            {badges.map((b) => (
              <Badge key={b} tone={b}>
                {b}
              </Badge>
            ))}
          </div>

          {directors && directors.length > 0 ? (
            <div className="mb-3 text-[13.5px] text-white/60">
              <span className="font-semibold text-white/80">{t('content.directedBy')}</span>{' '}
              {directors.map((d, i) => (
                <span key={d}>
                  {i > 0 ? ', ' : ''}
                  <button
                    type="button"
                    onClick={() => navigate({ to: '/person/$name', params: { name: d } })}
                    aria-label={t('person.viewWorks', { name: d })}
                    className="cursor-pointer bg-transparent p-0 text-inherit underline-offset-2 transition-colors hover:text-accent hover:underline"
                  >
                    {d}
                  </button>
                </span>
              ))}
            </div>
          ) : null}

          {tagline ? <p className="mb-3 text-[14px] italic text-white/50">{tagline}</p> : null}
          {overview ? (
            <p className="mb-5.5 text-[16px] leading-[1.6] text-white/82">{overview}</p>
          ) : null}

          <div className="mb-6.5 flex flex-wrap items-center gap-3.5">
            {primaryAction ??
              (onPlay ? (
                <Button onClick={onPlay} icon={<PlayIcon />}>
                  {playLabel ?? t('content.play')}
                </Button>
              ) : null)}
            {onToggleWatched ? (
              <button
                type="button"
                onClick={onToggleWatched}
                aria-pressed={watched ?? false}
                aria-label={watched ? t('content.markUnwatched') : t('content.markWatched')}
                title={watched ? t('content.watched') : t('content.markWatched')}
                className={`flex h-12.5 items-center gap-2 rounded-md border px-4 text-[14px] font-semibold transition-colors
                  ${
                    watched
                      ? 'border-accent bg-accent text-black hover:bg-accent/90'
                      : 'border-border-strong bg-white/10 text-text hover:bg-white/15'
                  }`}
              >
                <IconCheck size={19} stroke={2.4} />
                {watched ? t('content.watched') : t('content.markWatched')}
              </button>
            ) : null}
            {onToggleList ? (
              <button
                type="button"
                onClick={onToggleList}
                aria-pressed={inList ?? false}
                aria-label={inList ? t('content.removeFromList') : t('content.addToList')}
                title={inList ? t('content.inList') : t('content.addToList')}
                className={`flex h-12.5 w-12.5 items-center justify-center rounded-md border transition-colors
                  ${
                    inList
                      ? 'border-accent bg-accent-soft text-accent hover:bg-accent-soft/80'
                      : 'border-border-strong bg-white/10 text-text hover:bg-white/15'
                  }`}
              >
                {inList ? <IconCheck size={20} stroke={2.4} /> : <IconPlus size={20} stroke={2} />}
              </button>
            ) : null}
            {adminAction}
          </div>

          {audio != null || subtitles != null ? (
            <div className="flex flex-wrap gap-x-11 gap-y-4 border-t border-white/8 py-4.5">
              {audio != null ? <Field label={t('content.fieldAudio')} value={audio} /> : null}
              {subtitles != null ? (
                <Field label={t('content.fieldSubtitles')} value={subtitles} />
              ) : null}
            </div>
          ) : null}
          {unsupported ? <p className="mt-3.5 text-[13px] text-muted">{unsupported}</p> : null}
        </div>
      </div>
    </div>
  );
}

export interface SimilarItem {
  id: string;
  title: string;
  genre: string;
  /** When set, the show's season count the genre line is localized at render. */
  seasonCount?: number;
  badge: string | null;
  poster: string;
}

/** First + last initials, e.g. "George MacKay" → "GM". */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  const first = parts[0]?.[0] ?? '';
  const last = parts.length > 1 ? (parts.at(-1)?.[0] ?? '') : '';
  return (first + last).toUpperCase();
}

/** "Distribution" horizontal rail of initials avatars (matches the design;
 * the reference uses gradient initials, not photos). */
export function CastRail({ cast }: Readonly<{ cast: CastMember[] }>) {
  const t = useT();
  const navigate = useNavigate();
  if (cast.length === 0) return null;
  return (
    <section className="mt-10">
      <h2 className="mb-4.5 px-(--gutter-web) font-display text-[22px] font-bold tracking-[-.02em]">
        {t('content.cast')}
      </h2>
      <Rail gap={22} padded label={t('content.cast')}>
        {cast.map((p) => {
          const [g1, g2] = posterColors(p.name);
          const photo = imageUrl(p.profileUrl);
          return (
            <button
              key={`${p.name}-${p.character ?? ''}`}
              type="button"
              onClick={() => navigate({ to: '/person/$name', params: { name: p.name } })}
              aria-label={t('person.viewWorks', { name: p.name })}
              className="group w-28 shrink-0 cursor-pointer bg-transparent p-0 text-center outline-none transition-transform duration-200 hover:scale-[1.06] focus-visible:scale-[1.06]"
            >
              <Avatar className="mb-2.75 h-28 w-28 rounded-full shadow-[0_8px_22px_rgba(0,0,0,.45)] ring-accent transition-shadow duration-200 group-hover:ring-4 group-focus-visible:ring-4">
                {photo ? (
                  <AvatarImage
                    src={photo}
                    alt={p.name}
                    loading="lazy"
                    decoding="async"
                    draggable={false}
                  />
                ) : null}
                <AvatarFallback
                  className="font-display text-[36px] font-bold text-white/90"
                  style={{ background: `linear-gradient(158deg, ${g1}, ${g2})` }}
                >
                  <div className="absolute inset-0 bg-[radial-gradient(70%_60%_at_50%_22%,rgba(255,255,255,.2),transparent_60%)]" />
                  <span className="relative">{initials(p.name)}</span>
                </AvatarFallback>
              </Avatar>
              <div className="truncate text-[14px] font-semibold transition-colors group-hover:text-accent group-focus-visible:text-accent">
                {p.name}
              </div>
              {p.character ? (
                <div className="truncate text-[12px] font-medium text-white/45">{p.character}</div>
              ) : null}
            </button>
          );
        })}
      </Rail>
    </section>
  );
}

/** Horizontal "Titres similaires" rail of poster tiles. */
export function SimilarRail({
  title,
  items,
  onOpen,
}: Readonly<{ title: string; items: SimilarItem[]; onOpen: (id: string) => void }>) {
  if (items.length === 0) return null;
  return (
    <section className="mt-11">
      <h2 className="mb-4 px-(--gutter-web) font-display text-[22px] font-bold tracking-[-.02em]">
        {title}
      </h2>
      <Rail gap={18} padded label={title}>
        {items.map((m) => (
          <Poster
            key={m.id}
            title={m.title}
            genre={m.genre}
            colors={posterColors(m.id)}
            poster={m.poster}
            width={200}
            onClick={() => onOpen(m.id)}
          />
        ))}
      </Rail>
    </section>
  );
}
