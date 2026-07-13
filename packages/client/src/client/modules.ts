// The module registry (`GET /api/modules`): every module running on the server,
// each tagged with its admin `enabled` flag and the capabilities it provides. The
// admin console reads this to render engine add-flows data-driven, so disabling a
// module hides its add-UI and adding an engine needs no frontend change.

import type { ModuleInfo } from '../types';
import type { RequestContext } from './base';

/** Every module the server reports, with its enabled flag + provided capabilities
 * (each engine capability carries its add-form schema). */
export function listModules(ctx: RequestContext): Promise<ModuleInfo[]> {
  return ctx.json<ModuleInfo[]>('/modules');
}
