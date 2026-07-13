// Shim: the admin presentational primitives now live in `@luma/admin-kit` (the
// shared admin UI contract that module pages also use). Re-exported here so
// existing call sites keep importing from `#web/features/admin/ui`. New code
// (and every module page) should import from `@luma/admin-kit` directly.
export {
  Avatar,
  Button,
  C,
  Card,
  Disclosure,
  Field,
  FilterLabel,
  Modal,
  ModalActions,
  NumberField,
  Pill,
  ProgressBar,
  SegmentedControl,
  Section,
  Select,
  StatCard,
  TextInput,
  Toggle,
} from '@luma/admin-kit';
