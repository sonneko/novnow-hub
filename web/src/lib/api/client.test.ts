import { describe, expect, it } from 'vitest';
import { http, HttpResponse } from 'msw';
import { createApiClient } from './client';
import { mapApiFailure } from './errors';
import { projectPageFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('generated API client', () => {
  it('uses the contract-typed project list endpoint and query parameters', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects', ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('sort')).toBe('updated');
        expect(url.searchParams.get('limit')).toBe('20');
        return HttpResponse.json(projectPageFixture);
      }),
    );

    const client = createApiClient('http://hub.test/api/v1');
    const result = await client.GET('/projects', {
      params: { query: { sort: 'updated', limit: 20 } },
    });

    expect(result.error).toBeUndefined();
    expect(result.response.status).toBe(200);
    expect(result.data?.items[0]?.id).toBe('project-1');
  });

  it('maps RFC 9457 codes to API-independent UI failures', () => {
    expect(mapApiFailure({ code: 'not_found' }, 404)).toBe('not-found');
    expect(mapApiFailure({ code: 'rate_limited' }, 429)).toBe('rate-limited');
    expect(mapApiFailure({ code: 'upstream_unavailable' }, 503)).toBe(
      'upstream-unavailable',
    );
    expect(mapApiFailure(undefined, 401)).toBe('auth-required');
  });
});
