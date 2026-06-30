export interface AvatarProps {
  name?: string;
  size?: number;
  gradient?: string;
  radius?: string | number;
}

/** Profile / cast avatar gradient disc with initials (no photo needed). */
export function Avatar({ name = '', size = 64, gradient, radius = '50%' }: Readonly<AvatarProps>) {
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0])
    .join('')
    .toUpperCase();
  return (
    <div
      style={{
        width: size,
        height: size,
        borderRadius: radius,
        background: gradient ?? 'linear-gradient(135deg,#F4B642,#E8743B)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontFamily: 'var(--font-display)',
        fontWeight: 700,
        fontSize: Math.round(size * 0.42),
        color: 'rgba(255,255,255,.92)',
      }}
    >
      {initials}
    </div>
  );
}
