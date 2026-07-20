import { useT } from '@kroma/ui';
import type {
  ActionItem,
  ChoiceItem,
  RowBadge,
  SettingsEntry,
  ToggleItem,
} from '#tv/app/settings/items';
import { MenuRow } from './MenuRow';

/**
 * Render a settings menu from a declarative item list (see settings/items.ts).
 * Falsy entries (inline `cond && item`) and unavailable items are skipped, and
 * a choice row with fewer than two options hides itself. Each row is its own
 * component, so an item's `use()` hook lives in a stable component instance
 * and a parent's early return can never break hook order (the old React #300
 * switch-profile crash).
 */
export function SettingsRows({ items }: Readonly<{ items: readonly SettingsEntry[] }>) {
  return (
    <>
      {items.map((item) => {
        if (!item || (item.available && !item.available())) return null;
        if (item.kind === 'choice') return <ChoiceRow key={item.id} item={item} />;
        if (item.kind === 'toggle') return <ToggleRow key={item.id} item={item} />;
        return <ActionRow key={item.id} item={item} />;
      })}
    </>
  );
}

function Badge({ badge }: Readonly<{ badge: RowBadge }>) {
  const t = useT();
  return (
    <span
      className={`font-sans text-[15px] font-semibold ${
        badge.tone === 'success' ? 'text-success' : 'text-dim'
      }`}
    >
      {t(badge.label)}
    </span>
  );
}

function ChoiceRow({ item }: Readonly<{ item: ChoiceItem }>) {
  const t = useT();
  const [value, set] = item.use();
  const options = item.options();
  if (options.length < 2) return null;
  const cycle = () => {
    const next = options[(options.indexOf(value) + 1) % options.length];
    if (next) set(next);
  };
  const Icon = item.icon;
  return (
    <MenuRow icon={<Icon size={22} stroke={1.7} />} label={t(item.label)} onAct={cycle}>
      <span className="font-sans text-[16px] font-semibold text-accent">
        {t(item.valueLabel(value))}
      </span>
    </MenuRow>
  );
}

function ToggleRow({ item }: Readonly<{ item: ToggleItem }>) {
  const t = useT();
  const [on, set] = item.use();
  const Icon = item.icon;
  return (
    <MenuRow icon={<Icon size={22} stroke={1.7} />} label={t(item.label)} onAct={() => set(!on)}>
      <Badge
        badge={{ label: on ? 'profileMenu.on' : 'profileMenu.off', tone: on ? 'success' : 'dim' }}
      />
    </MenuRow>
  );
}

function ActionRow({ item }: Readonly<{ item: ActionItem }>) {
  const t = useT();
  const Icon = item.icon;
  return (
    <MenuRow icon={<Icon size={22} stroke={1.7} />} label={t(item.label)} onAct={item.run}>
      {item.badge ? <Badge badge={item.badge} /> : undefined}
    </MenuRow>
  );
}
