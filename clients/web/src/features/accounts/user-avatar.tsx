import { imageUrl } from '#web/shared/lib/api';
import { Avatar, AvatarFallback, AvatarImage } from '#web/shared/ui';

// Vivid two-stop gradients lifted from the LUMA design (LUMA.dc.html profiles),
// picked deterministically from a seed so a given account keeps its colour.
const GRADS = [
  'linear-gradient(135deg,#F4B642,#E8743B)', // amber → orange
  'linear-gradient(135deg,#3BC9DB,#3B82F6)', // cyan → blue
  'linear-gradient(135deg,#A855F7,#6366F1)', // purple → indigo
  'linear-gradient(135deg,#F472B6,#EC4899)', // pink
  'linear-gradient(135deg,#34D399,#10B981)', // green
];

function hashIndex(seed: string, n: number): number {
  let h = 0;
  for (let i = 0; i < seed.length; i += 1) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return h % n;
}

/** Deterministic avatar gradient for a seed (user id or name). */
export function avatarGradient(seed: string): string {
  return GRADS[hashIndex(seed || '?', GRADS.length)] as string;
}

/** Up-to-two-letter initials from a display name. */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase();
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase();
}

/**
 * Account avatar in the LUMA shape: a rounded-square gradient with Bricolage
 * initials, with the uploaded WebP photo layered over it once loaded (Radix
 * Avatar handles the swap, so SSR shows initials and the photo fades in).
 */
export function UserAvatar({
  name,
  avatarUrl,
  seed,
  size = 138,
  radius,
  className = '',
}: Readonly<{
  name: string;
  avatarUrl?: string | null;
  /** Stable colour seed (defaults to the name). */
  seed?: string;
  size?: number;
  radius?: number;
  className?: string;
}>) {
  const r = radius ?? Math.round(size * 0.13);
  return (
    <Avatar className={className} style={{ width: size, height: size, borderRadius: r }}>
      {avatarUrl ? (
        <AvatarImage src={imageUrl(avatarUrl) ?? undefined} alt="" loading="lazy" />
      ) : null}
      <AvatarFallback
        className="font-display font-bold text-white/90"
        style={{ background: avatarGradient(seed ?? name), fontSize: Math.round(size * 0.38) }}
      >
        {initials(name)}
      </AvatarFallback>
    </Avatar>
  );
}
