// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { reactivePref, useStoredPref } from './store';

describe('reactivePref', () => {
  it('reads the stored value and treats unknown ones as the fallback', () => {
    localStorage.setItem('kroma:test-read', 'b');
    expect(reactivePref('kroma:test-read', ['a', 'b'], 'a').get()).toBe('b');
    localStorage.setItem('kroma:test-junk', 'junk');
    expect(reactivePref('kroma:test-junk', ['a', 'b'], 'a').get()).toBe('a');
  });

  it('set persists, updates the snapshot and notifies each subscriber once', () => {
    const pref = reactivePref('kroma:test-set', ['x', 'y'], 'x');
    const seen = vi.fn();
    const off = pref.subscribe(seen);

    pref.set('y');
    expect(pref.get()).toBe('y');
    expect(localStorage.getItem('kroma:test-set')).toBe('y');
    expect(seen).toHaveBeenCalledTimes(1);

    pref.set('y'); // same-value write: no notification
    expect(seen).toHaveBeenCalledTimes(1);

    off();
    pref.set('x'); // unsubscribed: silent
    expect(seen).toHaveBeenCalledTimes(1);
  });

  it('useStoredPref re-renders the consumer when ANY writer sets the pref', () => {
    const pref = reactivePref('kroma:test-hook', ['x', 'y'], 'x');
    const { result } = renderHook(() => useStoredPref(pref));
    expect(result.current[0]).toBe('x');

    act(() => pref.set('y')); // an external writer, not the hook's own setter
    expect(result.current[0]).toBe('y');

    act(() => result.current[1]('x'));
    expect(result.current[0]).toBe('x');
    expect(pref.get()).toBe('x');
  });
});
