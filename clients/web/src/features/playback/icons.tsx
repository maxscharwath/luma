// Player icons backed by @tabler/icons-react, sized/stroked to match the LUMA
// player. Color comes from `currentColor`, so Tailwind text-* controls it.
import {
  IconChartBar as TbChartBar,
  IconCheck as TbCheck,
  IconChevronLeft as TbChevronLeft,
  IconChevronsLeft as TbChevronsLeft,
  IconChevronsRight as TbChevronsRight,
  IconMaximize as TbMaximize,
  IconMinimize as TbMinimize,
  IconPlayerPauseFilled as TbPause,
  IconPictureInPicture as TbPip,
  IconPlayerPlayFilled as TbPlay,
  IconPlayerStopFilled as TbStop,
  IconListDetails as TbTracks,
  IconVolume as TbVolume,
  IconVolumeOff as TbVolumeOff,
  IconX as TbX,
} from '@tabler/icons-react';

type P = { size?: number };

export const IconPlay = ({ size = 22 }: Readonly<P>) => <TbPlay size={size} />;
export const IconPause = ({ size = 22 }: Readonly<P>) => <TbPause size={size} />;
export const IconBack10 = ({ size = 22 }: Readonly<P>) => (
  <TbChevronsLeft size={size} stroke={1.8} />
);
export const IconFwd10 = ({ size = 22 }: Readonly<P>) => (
  <TbChevronsRight size={size} stroke={1.8} />
);
export const IconVolume = ({ size = 22 }: Readonly<P>) => <TbVolume size={size} stroke={1.8} />;
export const IconMute = ({ size = 22 }: Readonly<P>) => <TbVolumeOff size={size} stroke={1.8} />;
export const IconFullscreen = ({ size = 22 }: Readonly<P>) => (
  <TbMaximize size={size} stroke={1.8} />
);
export const IconFullscreenExit = ({ size = 22 }: Readonly<P>) => (
  <TbMinimize size={size} stroke={1.8} />
);
export const IconPip = ({ size = 22 }: Readonly<P>) => <TbPip size={size} stroke={1.8} />;
export const IconBack = ({ size = 22 }: Readonly<P>) => <TbChevronLeft size={size} stroke={1.8} />;
export const IconTracks = ({ size = 18 }: Readonly<P>) => <TbTracks size={size} stroke={1.8} />;
export const IconStats = ({ size = 22 }: Readonly<P>) => <TbChartBar size={size} stroke={1.8} />;
export const IconCheck = ({ size = 18 }: Readonly<P>) => <TbCheck size={size} stroke={1.8} />;
export const IconClose = ({ size = 18 }: Readonly<P>) => <TbX size={size} stroke={1.8} />;
export const IconStopped = ({ size = 52 }: Readonly<P>) => <TbStop size={size} />;
