/** Player control glyphs @tabler/icons-react, sized/stroked to match the TV
 * player. Color via `currentColor` unless overridden. */
import {
  IconCheck,
  IconChevronLeft,
  IconChevronsLeft,
  IconChevronsRight,
  IconList,
  IconPlayerPauseFilled,
  IconPlayerPlayFilled,
  IconPlayerStopFilled,
  IconSparkles,
  IconTrash,
} from '@tabler/icons-react';

export function PlayGlyph() {
  return <IconPlayerPlayFilled size={34} />;
}

export function PauseGlyph() {
  return <IconPlayerPauseFilled size={32} />;
}

export function RewindGlyph() {
  return <IconChevronsLeft size={30} stroke={1.8} />;
}

export function ForwardGlyph() {
  return <IconChevronsRight size={30} stroke={1.8} />;
}

export function TracksGlyph() {
  return <IconList size={22} stroke={1.8} />;
}

export function BackChevron() {
  return <IconChevronLeft size={20} stroke={2} />;
}

export function CheckGlyph() {
  return <IconCheck size={20} stroke={2.4} color="var(--kroma-accent)" />;
}

export function SparkleGlyph() {
  return <IconSparkles size={12} stroke={2} />;
}

export function TrashGlyph() {
  return <IconTrash size={18} stroke={1.8} />;
}

export function StopGlyph({ size = 52 }: Readonly<{ size?: number }>) {
  return <IconPlayerStopFilled size={size} />;
}
