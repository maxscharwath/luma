import { useT } from '@kroma/ui';
import type { CSSProperties } from 'react';
import { CLIENT_BUILD } from '#tv/app/clientBuild';
import { useConnection } from '#tv/app/providers/connection';

// Non-blocking banner shown when the connected server is older than this client
// build requires (see @kroma/core `checkServerCompat`). Deliberately PASSIVE - it
// takes no D-pad focus, so it never disrupts navigation - and it clears itself the
// moment the server is updated. Inline styles keep it legacy-TV safe.
const style: CSSProperties = {
  position: 'fixed',
  top: 0,
  left: 0,
  right: 0,
  zIndex: 9999,
  padding: '0.55em 1.2em',
  background: '#8a5a00',
  color: '#fff',
  fontSize: '1.05rem',
  textAlign: 'center',
};

export function CompatBanner() {
  const { compat, serverVersion } = useConnection();
  const t = useT();
  if (compat !== 'server-outdated') return null;
  return (
    <div style={style} role="status">
      {`⚠ ${t('compat.serverOutdated', { server: serverVersion ?? '?', client: CLIENT_BUILD.version })}`}
    </div>
  );
}
