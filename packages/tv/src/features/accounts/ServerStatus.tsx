import { Box, colors } from '@kroma/ui/kit';

/** Server-reachability dot shared by the profile picker and the add-profile
 * server list: green when up, red when down, a quiet grey while the first probe
 * is still pending (`online === undefined`). */
export function StatusDot({ online }: Readonly<{ online?: boolean }>) {
  const look = lookOf(online);
  return <Box w={10} h={10} shrink={0} radius="pill" bg={look.bg} style={look.glow} />;
}

function lookOf(online?: boolean) {
  if (online === undefined) return PENDING;
  return online ? UP : DOWN;
}

const PENDING = { bg: 'rgba(255, 255, 255, 0.25)', glow: null };
const UP = { bg: colors.success, glow: { boxShadow: '0 0 7px rgba(70, 208, 141, 0.75)' } };
const DOWN = { bg: colors.danger, glow: { boxShadow: '0 0 7px rgba(229, 57, 53, 0.75)' } };
