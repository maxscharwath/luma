// Profile avatars + the deterministic gradient palette and initials fallback,
// plus the padlock glyph used for PIN-protected profiles.

import { useState } from 'react';

// Vivid avatar gradients the same palette across web / TV profile pickers.
export const AVATAR_GRADS = [
  'linear-gradient(135deg,#F4B642,#E8743B)',
  'linear-gradient(135deg,#3BC9DB,#3B82F6)',
  'linear-gradient(135deg,#A855F7,#6366F1)',
  'linear-gradient(135deg,#F472B6,#EC4899)',
  'linear-gradient(135deg,#34D399,#10B981)',
];

/** Deterministic avatar gradient for a seed (user id), so a profile keeps its
 * colour everywhere. */
export function gradFor(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i += 1) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return AVATAR_GRADS[h % AVATAR_GRADS.length] as string;
}

/** 1–2 letter initials for an avatar fallback. */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase();
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase();
}

/** Rounded-square profile avatar uploaded photo when present, else a
 * deterministic gradient with the user's initials. Optional amber lock badge for
 * PIN-protected profiles. */
export function ProfileAvatar({
  name,
  seed,
  size,
  src,
  locked = false,
  radius,
}: Readonly<{
  name: string;
  seed: string;
  size: number;
  src?: string | null;
  locked?: boolean;
  radius?: number;
}>) {
  const [failed, setFailed] = useState(false);
  const showImg = Boolean(src) && !failed;
  const r = radius ?? Math.round(size * 0.16);
  return (
    <div
      className="relative flex items-center justify-center overflow-hidden font-display font-bold text-white/95"
      style={{
        width: size,
        height: size,
        borderRadius: r,
        background: gradFor(seed),
        fontSize: Math.round(size * 0.38),
        boxShadow: '0 16px 40px rgba(0,0,0,.45)',
      }}
    >
      {showImg ? (
        <img
          src={src ?? undefined}
          alt=""
          onError={() => setFailed(true)}
          className="absolute inset-0 h-full w-full object-cover"
        />
      ) : (
        initials(name)
      )}
      {locked ? (
        <span
          className="absolute bottom-2 right-2 flex items-center justify-center rounded-full bg-[rgba(10,10,12,0.8)] text-accent"
          style={{ width: Math.max(24, size * 0.2), height: Math.max(24, size * 0.2) }}
        >
          <LockGlyph size={Math.max(14, size * 0.11)} />
        </span>
      ) : null}
    </div>
  );
}

/** Padlock glyph (lock badge / PIN headers). */
export function LockGlyph({ size = 16 }: Readonly<{ size?: number }>) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M5 13a2 2 0 0 1 2 -2h10a2 2 0 0 1 2 2v6a2 2 0 0 1 -2 2h-10a2 2 0 0 1 -2 -2z" />
      <path d="M11 16a1 1 0 1 0 2 0a1 1 0 0 0 -2 0" />
      <path d="M8 11v-4a4 4 0 1 1 8 0v4" />
    </svg>
  );
}
