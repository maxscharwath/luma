import { describe, expect, it } from 'vitest';
import type { CreateReportBody } from '../types';
import type { RequestContext } from './base';
import {
  adminReports,
  createReport,
  deleteReport,
  dismissReport,
  listMyReports,
  reopenReport,
  resolveReport,
} from './reports';

function recordCtx() {
  const calls: { path: string; init?: RequestInit }[] = [];
  const ctx = {
    baseUrl: 'http://nas',
    json: async (path: string, init?: RequestInit) => {
      calls.push({ path, init });
      return {} as never;
    },
  } as unknown as RequestContext;
  return { ctx, calls };
}

describe('createReport / listMyReports', () => {
  it('POSTs the JSON body to /reports and lists own reports', () => {
    const { ctx, calls } = recordCtx();
    const body = {
      subjectKind: 'movie',
      subjectId: 'm1',
      category: 'audio',
    } as unknown as CreateReportBody;
    void createReport(ctx, body);
    void listMyReports(ctx);
    expect(calls[0]?.path).toBe('/reports');
    expect(calls[0]?.init?.method).toBe('POST');
    expect(JSON.parse(calls[0]?.init?.body as string)).toEqual(body);
    expect(calls[1]).toMatchObject({ path: '/reports/mine' });
  });
});

describe('adminReports query string', () => {
  it('omits empty filters and includes the set ones', () => {
    const { ctx, calls } = recordCtx();
    void adminReports(ctx);
    void adminReports(ctx, { status: 'open' });
    void adminReports(ctx, {
      status: 'resolved',
      category: 'video',
      kind: 'show',
      q: 'the office',
    });
    expect(calls[0]?.path).toBe('/admin/reports');
    expect(calls[1]?.path).toBe('/admin/reports?status=open');
    expect(calls[2]?.path).toBe(
      '/admin/reports?status=resolved&category=video&kind=show&q=the+office',
    );
  });
});

describe('triage endpoints', () => {
  it('use the encoded id and right verb', () => {
    const { ctx, calls } = recordCtx();
    void resolveReport(ctx, 'r 1');
    void dismissReport(ctx, 'r2');
    void reopenReport(ctx, 'r3');
    void deleteReport(ctx, 'r 4');
    expect(calls[0]).toMatchObject({
      path: '/admin/reports/r%201/resolve',
      init: { method: 'POST' },
    });
    expect(calls[1]).toMatchObject({ path: '/admin/reports/r2/dismiss', init: { method: 'POST' } });
    expect(calls[2]).toMatchObject({ path: '/admin/reports/r3/reopen', init: { method: 'POST' } });
    expect(calls[3]).toMatchObject({ path: '/admin/reports/r%204', init: { method: 'DELETE' } });
  });
});
