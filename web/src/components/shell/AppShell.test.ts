import { render, screen, within } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import AppShell from './AppShell.svelte';
import { messages } from '../../lib/i18n';
import { derivativeNodesFixture, projectFixture, setupInfoFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('AppShell M5 routes', () => {
  it('routes /derivatives to the project tree and marks the tab active', async () => {
    window.history.replaceState({}, '', `/derivatives/?projectId=${projectFixture.id}`);
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId', () => HttpResponse.json(projectFixture)),
      http.get('http://hub.test/api/v1/projects/:projectId/derivatives', () =>
        HttpResponse.json(derivativeNodesFixture),
      ),
    );

    render(AppShell, { page: { kind: 'derivatives' } });

    expect(await screen.findByText('原作・星を渡る')).toBeTruthy();
    const primaryNav = screen.getByRole('navigation', { name: messages.ja.nav.primary });
    expect(
      within(primaryNav).getByRole('link', {
        name: messages.ja.nav.derivatives,
        current: 'page',
      }),
    ).toBeTruthy();
  });

  it('routes /setup/resolve and reads the public URL from the u query parameter', async () => {
    window.history.replaceState(
      {},
      '',
      `/setup/resolve/?u=${encodeURIComponent(projectFixture.publishedUrl)}`,
    );
    let resolvedUrl: string | null = null;
    server.use(
      http.get('http://hub.test/api/v1/setup/resolve', ({ request }) => {
        resolvedUrl = new URL(request.url).searchParams.get('u');
        return HttpResponse.json(setupInfoFixture);
      }),
    );

    render(AppShell, { page: { kind: 'setupResolve' } });

    expect(await screen.findByRole('heading', { name: messages.ja.setupResolve.resultTitle })).toBeTruthy();
    expect(resolvedUrl).toBe(projectFixture.publishedUrl);
  });
});
