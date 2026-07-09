// File naming templates (Sonarr/Radarr-style) + the library rename tool.

import type {
  NamingTemplatesView,
  NamingView,
  OrganizePlan,
  OrganizeResult,
  SampleNames,
} from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Current templates + a rendered sample. */
export function adminNaming(ctx: RequestContext): Promise<NamingView> {
  return ctx.json<NamingView>('/admin/organize/naming');
}

/** Render a sample for the given (unsaved) templates, for the live preview. */
export function namingSample(
  ctx: RequestContext,
  templates: NamingTemplatesView,
): Promise<SampleNames> {
  return ctx.json<SampleNames>('/admin/organize/sample', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(templates),
  });
}

export function saveNaming(ctx: RequestContext, templates: NamingTemplatesView): Promise<void> {
  return ctx.json<void>('/admin/organize/naming', {
    method: 'PUT',
    headers: JSON_HEADERS,
    body: JSON.stringify(templates),
  });
}

/** Non-destructive: the list of library files that don't match the templates. */
export function organizePreview(ctx: RequestContext): Promise<OrganizePlan> {
  return ctx.json<OrganizePlan>('/admin/organize/preview');
}

/** Destructive: rename mismatched files to match the templates. */
export function organizeApply(ctx: RequestContext): Promise<OrganizeResult> {
  return ctx.json<OrganizeResult>('/admin/organize/apply', { method: 'POST' });
}
