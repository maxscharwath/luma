import { describe, expect, it } from 'vitest';
import { ringGeometry } from './ring';

describe('ringGeometry', () => {
  it('insets the radius by half the stroke so the ring is not clipped', () => {
    const g = ringGeometry({ value: 0, size: 22, stroke: 2.5 });
    expect(g.radius).toBe((22 - 2.5) / 2);
    expect(g.centre).toBe(11);
  });

  it('hides the whole circumference at 0 and none of it at 1', () => {
    expect(ringGeometry({ value: 0 }).dashOffset).toBeCloseTo(
      ringGeometry({ value: 0 }).circumference,
    );
    expect(ringGeometry({ value: 1 }).dashOffset).toBe(0);
  });

  it('hides half at the midpoint', () => {
    const g = ringGeometry({ value: 0.5 });
    expect(g.dashOffset).toBeCloseTo(g.circumference / 2);
  });

  it('clamps a value the caller has not sanitised', () => {
    expect(ringGeometry({ value: 2 }).dashOffset).toBe(0);
    const g = ringGeometry({ value: -1 });
    expect(g.dashOffset).toBeCloseTo(g.circumference);
    expect(ringGeometry({ value: Number.NaN }).dashOffset).toBeCloseTo(g.circumference);
  });
});
