export interface LogoProps {
  /** Height of the aperture mark in px; wordmark scales with it. */
  size?: number;
  /** Hide the "KROMA" wordmark, showing only the aperture mark. */
  markOnly?: boolean;
}

/** KROMA brand lockup a minimal amber "aperture" ring + centre dot beside the wordmark. */
export function Logo({ size = 26, markOnly = false }: Readonly<LogoProps>) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: size * 0.42 }}>
      <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <circle cx="12" cy="12" r="9" stroke="var(--kroma-accent)" strokeWidth="2.4" />
        <circle cx="12" cy="12" r="3.2" fill="var(--kroma-accent)" />
      </svg>
      {markOnly ? null : (
        <span
          style={{
            fontFamily: 'var(--font-display)',
            fontWeight: 800,
            fontSize: size * 0.74,
            letterSpacing: '.16em',
            color: 'var(--kroma-text)',
          }}
        >
          KROMA
        </span>
      )}
    </span>
  );
}
