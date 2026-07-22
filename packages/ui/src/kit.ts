/// <reference path="./types/react-native-tv.d.ts" />
/// <reference path="./types/react-native-web.d.ts" />
// @kroma/ui/kit: the universal component library.
//
// One source of components that renders natively on Apple TV / Android TV
// (React Native) and in the browser on Tizen / webOS / desktop (react-native-web).
// Nothing in here imports react-dom or reaches for the DOM outside a `.web.tsx`
// file, which is the ONLY place the two worlds differ; the bundlers pick per
// target (Metro takes the plain file, Vite the `.web` one).
//
// The package root (`@kroma/ui`) still exports the older DOM-only components the
// browser admin app uses. As screens move over, those disappear and this becomes
// the root. Nothing is duplicated between the two: they are disjoint sets.

// ---- focus ----
export type { FocusableProps, FocusState } from './focus/Focusable';
export { Focusable } from './focus/Focusable';
export { armPressGuard, clearPressGuard, PRESS_GUARD_MS, pressGuardActive } from './focus/guard';
export { useFocusNav } from './focus/nav';
export type { FocusEngine, FocusHostProps, FocusNavHandlers } from './focus/types';
// ---- media ----
export type { GridProps } from './media/Grid';
export { cellWidth, Grid } from './media/Grid';
export type { MediaCardProps } from './media/MediaCard';
export { CARD_SCRIM, MediaCard, tintGradient } from './media/MediaCard';
export type { PosterCardProps } from './media/PosterCard';
export { POSTER_SCRIM, PosterCard } from './media/PosterCard';
export type { RailProps } from './media/Rail';
export { Rail } from './media/Rail';
export type { GrowingCount } from './media/useGrowingCount';
export { useGrowingCount } from './media/useGrowingCount';
export type { WatchedBadgeProps } from './media/WatchedBadge';
export { WatchedBadge } from './media/WatchedBadge';
// ---- primitives ----
export type { AvatarProps } from './primitives/Avatar';
export { AVATAR_GRADIENT, Avatar, initialsOf } from './primitives/Avatar';
export type { BadgeProps, BadgeTone } from './primitives/Badge';
export { Badge } from './primitives/Badge';
export type { ButtonProps, ButtonSize, ButtonVariant } from './primitives/Button';
export { Button } from './primitives/Button';
export type { ChipProps } from './primitives/Chip';
export { Chip } from './primitives/Chip';
// ---- cross-platform CSS escape hatches ----
export { bgPosition, bgSize, gradient } from './primitives/css';
export type { ConfirmDialogProps, DialogProps } from './primitives/Dialog';
export { ConfirmDialog, Dialog, DialogFooter } from './primitives/Dialog';
export type { DividerProps } from './primitives/Divider';
export { Divider } from './primitives/Divider';
export type { EmptyStateProps } from './primitives/EmptyState';
export { EmptyState } from './primitives/EmptyState';
export type { Rect } from './primitives/focal';
export { coverRect, parsePosition } from './primitives/focal';
export type { IconName, IconProps } from './primitives/Icon';
export { Icon } from './primitives/Icon';
export { Img } from './primitives/Img';
export { DEFAULT_ICON_SIZE, DEFAULT_ICON_STROKE, ICON_NAMES } from './primitives/icons/glyph';
export type { ImgProps } from './primitives/img-types';
export { IMG_FADE_MS } from './primitives/img-types';
export type { ProgressProps } from './primitives/Progress';
export { clamp01, Progress } from './primitives/Progress';
export type { ProgressRingProps } from './primitives/ProgressRing';
export { ProgressRing } from './primitives/ProgressRing';
export type { RingGeometry, RingProps } from './primitives/ring';
export { RING_ROTATION, ringGeometry } from './primitives/ring';
export type { SkeletonProps } from './primitives/Skeleton';
export { Skeleton } from './primitives/Skeleton';
export type { SpinnerProps } from './primitives/Spinner';
export { Spinner } from './primitives/Spinner';
export type { TxtProps } from './primitives/Text';
export { Txt } from './primitives/Text';

// ---- stage ----
export type { TvStageProps } from './stage/TvStage';
export { TvStage } from './stage/TvStage';
// ---- style system ----
export type { BoxProps } from './system/Box';
export { Box, Column, Row, Spacer } from './system/Box';
export type { Align, BoxStyleProps, Justify, Spacing } from './system/boxStyle';
export { boxStyle, color } from './system/boxStyle';
export type { CompoundVariant, SvConfig, SvFn, VariantProps } from './system/sv';
export { sv } from './system/sv';
// ---- design tokens ----
export * from './tokens';
