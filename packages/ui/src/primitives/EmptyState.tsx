// <EmptyState>: the centred "nothing here" block. An icon, a headline, an
// optional hint and an optional action.

import type { ReactNode } from 'react';
import { Box } from '../system/Box';
import { Icon, type IconName } from './Icon';
import { Txt } from './Text';

export interface EmptyStateProps {
  icon: IconName;
  title: string;
  hint?: string;
  action?: ReactNode;
  /** 10-foot sizing: bigger glyph and type for a three-metre viewing distance. */
  tv?: boolean;
}

export function EmptyState({ icon, title, hint, action, tv = false }: Readonly<EmptyStateProps>) {
  return (
    <Box center mt={tv ? 96 : 64} gap={tv ? 16 : 8}>
      <Icon name={icon} size={tv ? 64 : 32} color="textDim" />
      <Txt variant={tv ? 'h2' : 'label'} style={{ textAlign: 'center' }}>
        {title}
      </Txt>
      {hint ? (
        <Txt
          variant={tv ? 'body' : 'meta'}
          color="textDim"
          style={{ textAlign: 'center', maxWidth: tv ? 720 : 400 }}
        >
          {hint}
        </Txt>
      ) : null}
      {action}
    </Box>
  );
}
