// <ProgressRing> on the browser targets: inline DOM svg, with a CSS transition
// on the dash offset so the arc eases rather than jumping between poll results.

import { RING_ROTATION, type RingProps, ringGeometry } from './ring';

export type { RingProps as ProgressRingProps } from './ring';

export function ProgressRing(props: Readonly<RingProps>) {
  const g = ringGeometry(props);
  return (
    <svg
      width={g.size}
      height={g.size}
      viewBox={`0 0 ${g.size} ${g.size}`}
      style={{ transform: `rotate(${RING_ROTATION})`, display: 'block', flexShrink: 0 }}
      aria-hidden="true"
      focusable="false"
    >
      <circle
        cx={g.centre}
        cy={g.centre}
        r={g.radius}
        fill="none"
        stroke={g.track}
        strokeWidth={g.stroke}
      />
      <circle
        cx={g.centre}
        cy={g.centre}
        r={g.radius}
        fill="none"
        stroke={g.fill}
        strokeWidth={g.stroke}
        strokeLinecap="round"
        strokeDasharray={g.circumference}
        strokeDashoffset={g.dashOffset}
        style={{ transition: 'stroke-dashoffset 260ms linear' }}
      />
    </svg>
  );
}
