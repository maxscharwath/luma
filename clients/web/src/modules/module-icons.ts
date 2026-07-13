// Resolves a module's declared `icon` name (a plain string in its manifest/nav)
// to a Tabler icon component. Keeps a small curated set relevant to modules;
// anything unknown falls back to the generic apps icon.

import {
  IconAntenna,
  IconApps,
  IconChartBar,
  IconCloud,
  IconDatabase,
  IconDeviceTv,
  IconDownload,
  IconLanguage,
  IconMagnet,
  IconMovie,
  IconPuzzle,
  IconRss,
  IconServer,
  IconShieldLock,
  IconSparkles,
  IconSubtask,
  type TablerIcon,
  IconWorld,
} from '@tabler/icons-react';

const ICONS: Record<string, TablerIcon> = {
  apps: IconApps,
  puzzle: IconPuzzle,
  download: IconDownload,
  magnet: IconMagnet,
  antenna: IconAntenna,
  rss: IconRss,
  movie: IconMovie,
  tv: IconDeviceTv,
  chart: IconChartBar,
  stats: IconChartBar,
  database: IconDatabase,
  world: IconWorld,
  cloud: IconCloud,
  ai: IconSparkles,
  vpn: IconShieldLock,
  server: IconServer,
  subtitles: IconLanguage,
  task: IconSubtask,
};

/** Resolve a module `icon` name to a Tabler icon component (fallback: apps). */
export function resolveModuleIcon(name?: string): TablerIcon {
  return (name ? ICONS[name] : undefined) ?? IconApps;
}
