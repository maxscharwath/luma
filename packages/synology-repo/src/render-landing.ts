/** The exact set of `{{TOKEN}}` placeholders the landing template expects. Both
 * the catalog generator and the live preview build one of these, so a token
 * rename becomes a compile error rather than a silently-empty render. */
export type Subs = {
  DNAME: string;
  ICON_FILE: string;
  VERSION: string;
  ARCH: string;
  DSM_FLOOR: string;
  CATALOG_URL: string;
  DOWNLOAD_URL: string;
  CHANNEL_SUFFIX: string;
  SOURCE_NAME: string;
  BETA_TRUST_HINT: string;
};

/** The channel-dependent (stable vs nightly) fields, so the nightly wording
 * lives in exactly one place. */
export function channelSubs(
  beta: boolean,
  dname: string,
): Pick<Subs, 'CHANNEL_SUFFIX' | 'SOURCE_NAME' | 'BETA_TRUST_HINT'> {
  return {
    CHANNEL_SUFFIX: beta ? ' (nightly)' : '',
    SOURCE_NAME: beta ? `${dname} nightly` : dname,
    BETA_TRUST_HINT: beta ? ', and enable <b>beta packages</b>' : '',
  };
}

/** Fill `{{TOKEN}}` placeholders in a landing-page template. Callers pass a
 * `Subs` (assignable to this record), so the token set is checked at the call site. */
export function renderLanding(template: string, subs: Record<string, string>): string {
  return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) => subs[key] ?? '');
}
