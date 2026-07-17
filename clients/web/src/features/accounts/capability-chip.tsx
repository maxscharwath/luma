import { capabilities, type PlaybackCapabilities } from '@kroma/core';
import { useT } from '@kroma/ui';
import * as Tooltip from '@radix-ui/react-tooltip';
import { useEffect, useState } from 'react';
import { Badge } from '#web/shared/ui';

/** Readout of what this device can direct-play, with a Radix tooltip showing the
 * detection method. Detection touches the DOM → client-only (neutral on the
 * server, filled in after mount to avoid a hydration mismatch). */
export function CapabilityChip() {
  const t = useT();
  const [caps, setCaps] = useState<PlaybackCapabilities | null>(null);
  useEffect(() => {
    setCaps(capabilities());
  }, []);

  return (
    <Tooltip.Provider delayDuration={150}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <span className="inline-flex cursor-default items-center gap-1.5">
            {caps?.hevc ? (
              <Badge tone="H.265">H.265 OK</Badge>
            ) : (
              <Badge tone="neutral">H.265 ✕</Badge>
            )}
            {caps?.hdr ? <Badge tone="HDR">HDR</Badge> : null}
            {caps?.av1 ? <Badge tone="info">AV1</Badge> : null}
          </span>
        </Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content
            sideOffset={6}
            className="z-50 rounded-md border border-border bg-surface-2 px-2.5 py-1.5 text-[12px] text-muted shadow-pop"
          >
            {caps ? t('common.detection', { source: caps.source }) : t('common.detecting')}
            <Tooltip.Arrow className="fill-surface-2" />
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}
