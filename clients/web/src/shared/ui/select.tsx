// A styled dropdown select built on Radix Select the app-wide replacement for
// native <select>. Renders as the design's chevron value-chip; the popup lists
// options with a check on the active one. Keyboard + a11y come from Radix.
//
// The styled control itself lives in @kroma/admin-kit (as `OptionSelect`, shared
// by the admin console AND every module page); re-exported here under the app's
// `Select`/`SelectProps` names so catalogue pages keep importing it from
// `#web/shared/ui` with no behavior change.
//
// Note: Radix forbids an empty-string option value. Use a non-empty sentinel
// (e.g. "none") for an "unset" choice and map it at the call site.

export {
  OptionSelect as Select,
  type OptionSelectProps as SelectProps,
  type SelectOption,
} from '@kroma/admin-kit';
