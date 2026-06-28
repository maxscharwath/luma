// A tiny declarative redirect system for the memory router. Instead of a nested
// `if (status) … else if (!user) …` ladder in an effect, navigation policy is a
// flat, ordered list of rules: "when <state>, the only allowed screens are
// <allow>; anything else redirects to <to>". First matching rule wins. Pure and
// trivially unit-testable — the effect just applies whatever it returns.
//
//   const target = resolveRedirect(RULES, state, nav.route.name);
//   if (target) nav.replace(target);

// `R` = every screen name; `T` = the screens a rule may redirect *to* (a subset —
// e.g. only param-less screens). Keeping them distinct lets callers redirect to a
// narrow, safe set while still allowing any screen to "stay".
export interface RedirectRule<S, R extends string, T extends R = R> {
  /** Does this rule govern the current state? */
  when: (state: S) => boolean;
  /** Where to send the user when they're on a screen this rule disallows. */
  to: T;
  /** Screens that are allowed to stay put under this rule. */
  allow: readonly R[];
}

/**
 * Resolve the screen the user must be on. Returns the redirect target, or `null`
 * when the current screen is already allowed (or no rule applies).
 */
export function resolveRedirect<S, R extends string, T extends R>(
  rules: readonly RedirectRule<S, R, T>[],
  state: S,
  current: R,
): T | null {
  for (const rule of rules) {
    if (rule.when(state)) return rule.allow.includes(current) ? null : rule.to;
  }
  return null;
}
