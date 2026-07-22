import type { CastMember } from '@kroma/core';
import { useLocale, useT } from '@kroma/ui';
import {
  AVATAR_GRADIENTS,
  Avatar,
  Box,
  Button,
  Focusable,
  Icon,
  IconButton,
  Rail,
  radius,
  Txt,
} from '@kroma/ui/kit';
import { useClient, useNav } from '#tv/app/router';

/** Wall-clock time `runtimeMs` from now, in the active locale: French 24-hour
 * "21h32", else a localised 12/24-hour time. Empty when the runtime is unknown. */
export function endsAtClock(runtimeMs?: number | null, locale?: string): string {
  if (!runtimeMs || runtimeMs <= 0) return '';
  const d = new Date(Date.now() + runtimeMs);
  if (locale === 'en') {
    return d.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
  }
  return `${d.getHours()}h${d.getMinutes().toString().padStart(2, '0')}`;
}

/** "Se termine à 21h32 si vous lancez maintenant", only when a runtime is known. */
export function EndsAtHint({ runtimeMs }: Readonly<{ runtimeMs?: number | null }>) {
  const t = useT();
  const locale = useLocale();
  const at = endsAtClock(runtimeMs, locale);
  if (!at) return null;
  return (
    <Box row align="center" gap={9} mt={12}>
      <Icon name="clock" size={16} stroke={1.8} color="accent" />
      <Txt style={SECTION_LABEL_SM} color="rgba(244, 243, 240, 0.55)">
        {t('content.endsAt', { time: at })}
      </Txt>
    </Box>
  );
}

const SECTION_LABEL_SM = { fontSize: 15, fontWeight: '600' as const };

const SECTION_LABEL = {
  fontSize: 15,
  fontWeight: '700' as const,
  letterSpacing: 0.6,
  textTransform: 'uppercase' as const,
};

/** "Distribution": top-billed cast. Shows the real TMDB headshot when present,
 * else a per-position gradient with initials (varied by index so neighbours never
 * collide). Each face is focusable and opens that person's titles. */
export function CastRow({ cast }: Readonly<{ cast?: CastMember[] | null }>) {
  const t = useT();
  const client = useClient();
  const nav = useNav();
  if (!cast || cast.length === 0) return null;
  return (
    <Box mt={32} gap={16}>
      <Txt style={SECTION_LABEL} color="rgba(244, 243, 240, 0.55)">
        {t('content.cast')}
      </Txt>
      <Rail inset={6} gap={24}>
        {cast.slice(0, 16).map((p, i) => (
          <CastFace
            key={`${p.name}-${p.character ?? ''}`}
            name={p.name}
            character={p.character}
            photo={client.resolveArt(p.profileUrl)}
            gradient={CAST_GRADIENTS[i % CAST_GRADIENTS.length] as string}
            label={t('person.viewWorks', { name: p.name })}
            onPress={() => nav.go('person', { name: p.name })}
          />
        ))}
      </Rail>
    </Box>
  );
}

/** One cast face. The ring is drawn on the AVATAR, never as a square box around
 * the whole card, and the name tints amber alongside it: `ring={false}` on the
 * focusable is what lets the card own that treatment. */
function CastFace({
  name,
  character,
  photo,
  gradient,
  label,
  onPress,
}: Readonly<{
  name: string;
  character?: string | null;
  photo: string | null;
  gradient: string;
  label: string;
  onPress: () => void;
}>) {
  return (
    <Focusable onPress={onPress} label={label} focusScale={1.06} ring={false} style={FACE}>
      {({ focused }) => (
        <>
          <Box radius="pill" style={focused ? RING : null}>
            <Avatar
              name={name}
              src={photo}
              gradient={gradient}
              size={120}
              radius={radius.pill}
              shadow={false}
            />
          </Box>
          <Txt
            lines={1}
            style={{ fontSize: 16, fontWeight: '600', textAlign: 'center' }}
            color={focused ? 'accent' : 'text'}
          >
            {name}
          </Txt>
          {character ? (
            <Txt
              lines={1}
              style={{ fontSize: 14, fontWeight: '500', textAlign: 'center' }}
              color="textDim"
            >
              {character}
            </Txt>
          ) : null}
        </>
      )}
    </Focusable>
  );
}

const FACE = { width: 120, flexShrink: 0, alignItems: 'center', gap: 6 } as const;
const RING = {
  boxShadow: '0 0 0 4px #F4B642, 0 10px 28px rgba(0, 0, 0, 0.5)',
  borderRadius: radius.pill,
} as const;

/** My-list toggle. */
export function ListButton({
  inList,
  onToggle,
}: Readonly<{ inList: boolean; onToggle: () => void }>) {
  const t = useT();
  return (
    <Button
      variant="outline"
      size="lg"
      active={inList}
      icon={inList ? 'check' : 'plus'}
      label={inList ? t('content.inList') : t('content.addToList')}
      onPress={onToggle}
    />
  );
}

/** Watched toggle: marks a title seen / unseen (persisted via the watched API). */
export function WatchedButton({
  watched,
  onToggle,
}: Readonly<{ watched: boolean; onToggle: () => void }>) {
  const t = useT();
  return (
    <Button
      variant="outline"
      size="lg"
      active={watched}
      icon="check"
      label={watched ? t('content.watched') : t('content.markWatched')}
      onPress={onToggle}
    />
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
    <IconButton
      icon={muted ? 'volume-off' : 'volume'}
      glyph={24}
      size={60}
      label={label}
      onPress={onToggle}
    />
  );
}

// Cast-circle gradients, cycled by position so adjacent faces never share a colour.
const CAST_GRADIENTS = [...AVATAR_GRADIENTS, 'linear-gradient(135deg, #FBBF24, #F97316)'];
