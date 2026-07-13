/** Server-reachability dot shared by the profile picker and the add-profile
 * server list: green when up, a pulsing red when down, a quiet grey while the
 * first probe is still pending (`online === undefined`). */
export function StatusDot({ online }: Readonly<{ online?: boolean }>) {
  let cls: string;
  if (online === undefined) cls = 'bg-[rgba(255,255,255,0.25)]';
  else if (online) cls = 'bg-success shadow-[0_0_7px_rgba(70,208,141,0.75)]';
  else cls = 'animate-pulse bg-danger shadow-[0_0_7px_rgba(229,57,53,0.75)]';
  return <span className={`size-2.5 flex-none rounded-full ${cls}`} />;
}
