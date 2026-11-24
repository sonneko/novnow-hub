import { render, screen, waitFor, within } from '@testing-library/svelte';
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

  it('shows the notification bell with an unread badge from the contract header', async () => {
    window.history.replaceState({}, '', '/projects/');
    server.use(
      http.get('http://hub.test/api/v1/notifications', () =>
        new HttpResponse(JSON.stringify({ items: [], nextCursor: null }), {
          status: 200,
          headers: { 'X-Unread-Count': '2' },
        }),
      ),
    );

    render(AppShell, { page: { kind: 'projects' } });

    const bell = await screen.findByRole('link', { name: messages.ja.controls.notifications });
    expect(bell.getAttribute('href')).toBe('/notifications/');
    await waitFor(() =>
      expect(within(bell).getByText('2')).toBeTruthy(),
    );
  });
});
