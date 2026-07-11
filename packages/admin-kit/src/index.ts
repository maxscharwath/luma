// @luma/admin-kit: the admin UI contract. The presentational primitives, hooks,
// and host-context provider that admin pages render with, whether built into the
// web app or contributed by a module. A module ui/ package imports everything it
// needs for a full admin page from here, so it never reaches into app internals.

export { AdminKitProvider, useAdminKit, resolveImageUrl, type AdminKitValue } from './context';
export {
  avatarGradient,
  decimal,
  formatBytes,
  hue,
  initial,
} from './format';
export {
  C,
  Card,
  FilterLabel,
  Pill,
  ProgressBar,
  Section,
  StatCard,
  Toggle,
  Avatar,
} from './primitives';
export { Button, Disclosure, NumberField, SegmentedControl } from './controls';
export {
  Field,
  Modal,
  ModalActions,
  OptionSelect,
  Select,
  TextInput,
  type OptionSelectProps,
  type SelectOption,
} from './forms';
export { HeaderAction, PageHeader, PAGE_SUBTITLE, PAGE_TITLE } from './header';
export { Denied, isAnyAdmin, useAsyncAction, useCap, usePoll } from './hooks';
export { CardSkeleton, EmptyState, Skeleton, TableSkeleton } from './feedback';
export { SettingsView } from './settings';
