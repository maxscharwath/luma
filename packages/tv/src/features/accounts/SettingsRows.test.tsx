// @vitest-environment jsdom
import { I18nProvider } from '@kroma/ui';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actionItem, choiceItem, type SettingsEntry, toggleItem } from '#tv/app/settings/items';
import { reactivePref, useStoredPref } from '#tv/app/settings/store';
import { SettingsRows } from './SettingsRows';

const Ic = () => null;

function show(items: readonly SettingsEntry[]) {
  return render(
    <I18nProvider locale="en">
      <SettingsRows items={items} />
    </I18nProvider>,
  );
}

afterEach(cleanup);

describe('SettingsRows', () => {
  it('cycles a choice row through its options on activation', () => {
    const pref = reactivePref('kroma:test-rows-cycle', ['abc', 'azerty'], 'abc');
    show([
      choiceItem({
        id: 'kbd',
        level: 'device',
        label: 'keyboardLayout.title',
        icon: Ic,
        options: () => ['abc', 'azerty'] as const,
        valueLabel: () => 'keyboardLayout.title',
        use: () => useStoredPref(pref),
      }),
    ]);
    const row = screen.getByRole('button');
    fireEvent.click(row);
    expect(pref.get()).toBe('azerty');
    fireEvent.click(row); // wraps back around
    expect(pref.get()).toBe('abc');
  });

  it('hides a choice row with fewer than two options', () => {
    const pref = reactivePref('kroma:test-rows-single', ['abc'], 'abc');
    show([
      choiceItem({
        id: 'single',
        level: 'device',
        label: 'keyboardLayout.title',
        icon: Ic,
        options: () => ['abc'] as const,
        valueLabel: () => 'keyboardLayout.title',
        use: () => useStoredPref(pref),
      }),
    ]);
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('flips a toggle and runs an action', () => {
    const setToggle = vi.fn();
    const run = vi.fn();
    show([
      toggleItem({
        id: 'gpu',
        level: 'shell',
        label: 'profileMenu.gpuRendering',
        icon: Ic,
        use: () => [false, setToggle] as const,
      }),
      actionItem({ id: 'quit', label: 'profileMenu.quitApp', icon: Ic, run }),
    ]);
    const [toggle, action] = screen.getAllByRole('button');
    if (!toggle || !action) throw new Error('expected a toggle and an action row');
    fireEvent.click(toggle);
    expect(setToggle).toHaveBeenCalledWith(true);
    fireEvent.click(action);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it('skips unavailable items and falsy inline entries', () => {
    const run = vi.fn();
    show([
      false,
      null,
      undefined,
      actionItem({
        id: 'gated',
        label: 'profileMenu.quitApp',
        icon: Ic,
        available: () => false,
        run,
      }),
      actionItem({ id: 'shown', label: 'profileMenu.quitApp', icon: Ic, run }),
    ]);
    expect(screen.getAllByRole('button')).toHaveLength(1);
  });
});
