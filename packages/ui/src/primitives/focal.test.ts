import { describe, expect, it } from 'vitest';
import { coverRect, parsePosition } from './focal';

describe('parsePosition', () => {
  it('reads the percentage pairs the design uses', () => {
    expect(parsePosition('50% 28%')).toEqual({ x: 0.5, y: 0.28 });
    expect(parsePosition('50% 30%')).toEqual({ x: 0.5, y: 0.3 });
    expect(parsePosition('50% 50%')).toEqual({ x: 0.5, y: 0.5 });
  });

  it('accepts the CSS keywords', () => {
    expect(parsePosition('left top')).toEqual({ x: 0, y: 0 });
    expect(parsePosition('right bottom')).toEqual({ x: 1, y: 1 });
    expect(parsePosition('center center')).toEqual({ x: 0.5, y: 0.5 });
  });

  it('centres the missing axis and tolerates junk', () => {
    expect(parsePosition('25%')).toEqual({ x: 0.25, y: 0.5 });
    expect(parsePosition('')).toEqual({ x: 0.5, y: 0.5 });
    expect(parsePosition('nonsense here')).toEqual({ x: 0.5, y: 0.5 });
  });
});

describe('coverRect', () => {
  const box = { width: 1920, height: 640 };

  it('is null until both the box and the artwork are measured', () => {
    expect(coverRect(null, { width: 1920, height: 1080 }, { x: 0.5, y: 0.3 })).toBeNull();
    expect(coverRect(box, null, { x: 0.5, y: 0.3 })).toBeNull();
    expect(coverRect({ width: 0, height: 0 }, { width: 16, height: 9 }, { x: 0, y: 0 })).toBeNull();
  });

  it('scales a 16:9 backdrop to cover a letterbox hero and anchors the upper third', () => {
    // 1920x1080 art into a 1920x640 hero: no horizontal overflow, 440px vertical.
    const r = coverRect(box, { width: 1920, height: 1080 }, { x: 0.5, y: 0.3 });
    expect(r).toEqual({ left: 0, top: -132, width: 1920, height: 1080 });
    // 30% of the 440px overflow is hidden above, so the face sits high in frame.
    expect(r && -r.top / (r.height - box.height)).toBeCloseTo(0.3);
  });

  it('centres by default, matching object-position 50% 50%', () => {
    const r = coverRect(box, { width: 1920, height: 1080 }, { x: 0.5, y: 0.5 });
    expect(r?.top).toBe(-220);
  });

  it('leaves an exactly-fitting axis alone', () => {
    // A 16:9 poster into a 16:9 tile overflows on neither axis, so no offset.
    const r = coverRect(
      { width: 328, height: 184.5 },
      { width: 1280, height: 720 },
      { x: 0.5, y: 0.28 },
    );
    expect(r?.left).toBeCloseTo(0);
    expect(r?.top).toBeCloseTo(0);
  });

  it('overflows horizontally when the artwork is wider than the box', () => {
    const r = coverRect(
      { width: 200, height: 300 },
      { width: 1600, height: 900 },
      { x: 0.5, y: 0.5 },
    );
    // Cover a 2:3 tile with a 16:9 still: scale by height, crop the sides.
    expect(r?.height).toBeCloseTo(300);
    expect(r?.width).toBeCloseTo(533.33, 1);
    expect(r?.left).toBeCloseTo(-166.67, 1);
  });
});
