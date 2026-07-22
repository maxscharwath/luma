// App iconography: the same Tabler set the web client uses, wrapped under the
// app's semantic names so call sites stay stable.

import IconAlertCircle from '@tabler/icons-react-native/dist/esm/icons/IconAlertCircle.mjs';
import IconBadgeCc from '@tabler/icons-react-native/dist/esm/icons/IconBadgeCc.mjs';
import IconCheck from '@tabler/icons-react-native/dist/esm/icons/IconCheck.mjs';
import IconChevronDown from '@tabler/icons-react-native/dist/esm/icons/IconChevronDown.mjs';
import IconChevronLeft from '@tabler/icons-react-native/dist/esm/icons/IconChevronLeft.mjs';
import IconChevronRight from '@tabler/icons-react-native/dist/esm/icons/IconChevronRight.mjs';
import IconDeviceTv from '@tabler/icons-react-native/dist/esm/icons/IconDeviceTv.mjs';
import IconDownload from '@tabler/icons-react-native/dist/esm/icons/IconDownload.mjs';
import IconEye from '@tabler/icons-react-native/dist/esm/icons/IconEye.mjs';
import IconEyeCheck from '@tabler/icons-react-native/dist/esm/icons/IconEyeCheck.mjs';
import IconFlag from '@tabler/icons-react-native/dist/esm/icons/IconFlag.mjs';
import IconLock from '@tabler/icons-react-native/dist/esm/icons/IconLock.mjs';
import IconLogout from '@tabler/icons-react-native/dist/esm/icons/IconLogout.mjs';
import IconPencil from '@tabler/icons-react-native/dist/esm/icons/IconPencil.mjs';
import IconPictureInPicture from '@tabler/icons-react-native/dist/esm/icons/IconPictureInPicture.mjs';
import IconPlayerPauseFilled from '@tabler/icons-react-native/dist/esm/icons/IconPlayerPauseFilled.mjs';
import IconPlayerPlayFilled from '@tabler/icons-react-native/dist/esm/icons/IconPlayerPlayFilled.mjs';
import IconPlus from '@tabler/icons-react-native/dist/esm/icons/IconPlus.mjs';
import IconRewindBackward10 from '@tabler/icons-react-native/dist/esm/icons/IconRewindBackward10.mjs';
import IconRewindForward10 from '@tabler/icons-react-native/dist/esm/icons/IconRewindForward10.mjs';
import IconScan from '@tabler/icons-react-native/dist/esm/icons/IconScan.mjs';
import IconSettings from '@tabler/icons-react-native/dist/esm/icons/IconSettings.mjs';
import IconTrash from '@tabler/icons-react-native/dist/esm/icons/IconTrash.mjs';
import IconUsers from '@tabler/icons-react-native/dist/esm/icons/IconUsers.mjs';
import type { ColorValue } from 'react-native';

interface IconProps {
  size?: number;
  color?: ColorValue;
}

const IVORY = '#F4F3F0';

export function PlayIcon({ size = 34, color = IVORY }: Readonly<IconProps>) {
  return <IconPlayerPlayFilled width={size} height={size} color={color} fill={color} />;
}

export function PauseIcon({ size = 34, color = IVORY }: Readonly<IconProps>) {
  return <IconPlayerPauseFilled width={size} height={size} color={color} fill={color} />;
}

export function BackIcon({ size = 26, color = IVORY }: Readonly<IconProps>) {
  return <IconChevronLeft width={size} height={size} color={color} strokeWidth={2.4} />;
}

export function Back10Icon({ size = 32, color = IVORY }: Readonly<IconProps>) {
  return <IconRewindBackward10 width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function Forward10Icon({ size = 32, color = IVORY }: Readonly<IconProps>) {
  return <IconRewindForward10 width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function TracksIcon({ size = 26, color = IVORY }: Readonly<IconProps>) {
  return <IconBadgeCc width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function GearIcon({ size = 24, color = IVORY }: Readonly<IconProps>) {
  return <IconSettings width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function CheckIcon({ size = 20, color = IVORY }: Readonly<IconProps>) {
  return <IconCheck width={size} height={size} color={color} strokeWidth={2.4} />;
}

export function DownloadIcon({ size = 22, color = IVORY }: Readonly<IconProps>) {
  return <IconDownload width={size} height={size} color={color} strokeWidth={2} />;
}

export function TrashIcon({ size = 20, color = IVORY }: Readonly<IconProps>) {
  return <IconTrash width={size} height={size} color={color} strokeWidth={2} />;
}

export function PlusIcon({ size = 22, color = IVORY }: Readonly<IconProps>) {
  return <IconPlus width={size} height={size} color={color} strokeWidth={2.2} />;
}

export function TvIcon({ size = 22, color = IVORY }: Readonly<IconProps>) {
  return <IconDeviceTv width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function PipIcon({ size = 24, color = IVORY }: Readonly<IconProps>) {
  return <IconPictureInPicture width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function ChevronDownIcon({ size = 18, color = IVORY }: Readonly<IconProps>) {
  return <IconChevronDown width={size} height={size} color={color} strokeWidth={2.4} />;
}

export function EyeIcon({ size = 22, color = IVORY }: Readonly<IconProps>) {
  return <IconEye width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function EyeCheckIcon({ size = 22, color = IVORY }: Readonly<IconProps>) {
  return <IconEyeCheck width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function FlagIcon({ size = 22, color = IVORY }: Readonly<IconProps>) {
  return <IconFlag width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function LockIcon({ size = 18, color = IVORY }: Readonly<IconProps>) {
  return <IconLock width={size} height={size} color={color} strokeWidth={2.2} />;
}

export function UsersIcon({ size = 22, color = IVORY }: Readonly<IconProps>) {
  return <IconUsers width={size} height={size} color={color} strokeWidth={2} />;
}

export function AlertIcon({ size = 20, color = IVORY }: Readonly<IconProps>) {
  return <IconAlertCircle width={size} height={size} color={color} strokeWidth={2} />;
}

export function ScanIcon({ size = 24, color = IVORY }: Readonly<IconProps>) {
  return <IconScan width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function ChevronRightIcon({ size = 18, color = IVORY }: Readonly<IconProps>) {
  return <IconChevronRight width={size} height={size} color={color} strokeWidth={2.2} />;
}

export function PencilIcon({ size = 18, color = IVORY }: Readonly<IconProps>) {
  return <IconPencil width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function LogoutIcon({ size = 20, color = IVORY }: Readonly<IconProps>) {
  return <IconLogout width={size} height={size} color={color} strokeWidth={1.8} />;
}
