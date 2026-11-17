import { fireEvent, render, screen } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import SetupResolvePage from './SetupResolvePage.svelte';
import { messages } from '../../lib/i18n';
import { readerPageHref } from '../../lib/links';
import { projectFixture, setupInfoFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('SetupResolvePage', () => {
  it('shows the resolved setup details and browser handoff from the mock API', async () => {
    let requestedUrl: string | null = null;
    server.use(
      http.get('http://hub.test/api/v1/setup/resolve', ({ request }) => {
        requestedUrl = new URL(request.url).searchParams.get('u');
        return HttpResponse.json(setupInfoFixture);
      }),
    );

    render(SetupResolvePage, {
      setupUrl: projectFixture.publishedUrl,
      m: messages.ja,
    });

    expect(
      await screen.findByRole('heading', {
        name: messages.ja.setupResolve.resultTitle,
        level: 2,
      }),
    ).toBeTruthy();
    expect(screen.getByText(setupInfoFixture.projectId)).toBeTruthy();
    expect(screen.getByText(setupInfoFixture.slug)).toBeTruthy();
    expect(screen.getByText(setupInfoFixture.snapshotVersion)).toBeTruthy();
    expect(screen.getByText(setupInfoFixture.novnow.minVersion)).toBeTruthy();
    expect(requestedUrl).toBe(projectFixture.publishedUrl);

    const manualLink = screen.getByRole('link', {
      name: setupInfoFixture.acquisition.url,
    });
    expect(manualLink.getAttribute('href')).toBe(setupInfoFixture.acquisition.url);
    expect(
      screen.getByRole('link', { name: messages.ja.setupResolve.openInBrowser }).getAttribute('href'),
    ).toBe(readerPageHref(setupInfoFixture.projectId));
    expect(screen.getAllByRole('listitem')).toHaveLength(messages.ja.setupResolve.steps.length);
  });

  it('lets a visitor paste a setup URL and resolves it without changing the API contract', async () => {
    let requestedUrl: string | null = null;
    server.use(
      http.get('http://hub.test/api/v1/setup/resolve', ({ request }) => {
        requestedUrl = new URL(request.url).searchParams.get('u');
        return HttpResponse.json(setupInfoFixture);
      }),
    );

    render(SetupResolvePage, {
      setupUrl: '',
      m: messages.en,
    });

    fireEvent.input(screen.getByLabelText(messages.en.setupResolve.inputLabel), {
      target: { value: projectFixture.publishedUrl },
    });
    fireEvent.click(screen.getByRole('button', { name: messages.en.setupResolve.submit }));

    expect(await screen.findByRole('heading', { name: 'Setup information', level: 2 })).toBeTruthy();
    expect(requestedUrl).toBe(projectFixture.publishedUrl);
    expect(screen.getByRole('link', { name: 'Open novnow in the browser' })).toBeTruthy();
  });
});
