// The bottom hint strip shared by Home and the browse grids: three short remote
// hints over a gradient that fades into the page. Purely decorative, so it never
// takes pointer events and never joins the focus set.

import type { MessageKey } from '@kroma/core';
import { useT } from '@kroma/ui';
import { Box, gradient, Txt } from '@kroma/ui/kit';

const HINT = { fontSize: 13, fontWeight: '600' as const };

/** `strength` matches the design: Home fades from 0.8, the grids from 0.85 (they
 * have a denser field of tiles running under the strip). */
export function HintBar({
  browseKey,
  strength = 0.8,
}: Readonly<{ browseKey: MessageKey; strength?: number }>) {
  const t = useT();
  return (
    <Box
      absolute
      left={0}
      right={0}
      bottom={0}
      row
      center
      gap={30}
      p={16}
      pointerEvents="none"
      style={gradient(`linear-gradient(0deg, rgba(10,10,12,${strength}), transparent)`)}
    >
      <Txt style={HINT} color="textDim">
        {t(browseKey)}
      </Txt>
      <Txt style={HINT} color="textDim">
        {t('content.hintRows')}
      </Txt>
      <Txt style={HINT} color="textDim">
        <Txt style={{ ...HINT, fontWeight: '700' }} color="accent">
          {t('content.hintOk')}
        </Txt>
        {` ${t('content.hintOpen')}`}
      </Txt>
    </Box>
  );
}
