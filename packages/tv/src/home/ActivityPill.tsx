import type { Activity } from '@luma/core';
import { useT } from '@luma/ui';

/** Live scan/enrichment status pill, shown only while work is in progress. */
export function ActivityPill({ activity }: Readonly<{ activity: Activity | null }>) {
  const t = useT();
  let label: string | null = null;
  if (activity) {
    if (activity.scanning || activity.phase === 'scanning') label = t('player.activityScanning');
    else if (activity.phase === 'enriching' && activity.enrichTotal > 0)
      label = t('player.activityArtwork', {
        done: activity.enrichDone,
        total: activity.enrichTotal,
      });
  }
  if (!label) return null;
  return (
    <span className="inline-flex items-center gap-2.5 rounded-full border border-border bg-[rgba(10,10,12,0.5)] px-4.5 py-2 font-sans text-[15px] font-semibold text-muted tabular-nums backdrop-blur-[10px]">
      <span className="h-2.5 w-2.5 rounded-full bg-accent shadow-[0_6px_22px_rgba(242,180,66,0.4)] animate-[luma-breathe_1.4s_var(--ease-out)_infinite]" />
      {label}
    </span>
  );
}
