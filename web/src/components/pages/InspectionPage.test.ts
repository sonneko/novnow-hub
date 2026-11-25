import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import InspectionPage from './InspectionPage.svelte';
import { messages } from '../../lib/i18n';
import { derivativesPageHref, inspectionPageHref, projectPageHref } from '../../lib/links';
import { projectFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('InspectionPage', () => {
  it('renders the inspection tabs and runs a diagnostics job to a succeeded result', async () => {
    const jobId = 'job_1';
    server.use(
      http.post('http://hub.test/api/v1/projects/:projectId/diagnostics', () =>
        HttpResponse.json({ jobId, status: 'pending' }, { status: 202 }),
      ),
      http.get('http://hub.test/api/v1/jobs/:jobId', () =>
        HttpResponse.json({
          id: jobId,
          kind: 'diagnostics',
          status: 'succeeded',
          result: {
            diagnostics: {
              ok: true,
              errors: [],
              warnings: [{ path: 'words.md', code: 'novnow_check', message: '注意: 単語が少ない' }],
              checkedAt: '2026-01-01T00:00:00Z',
            },
          },
          error: null,
          createdAt: '2026-01-01T00:00:00Z',
          finishedAt: '2026-01-01T00:00:01Z',
        }),
      ),
    );

    render(InspectionPage, {
      projectId: projectFixture.id,
      locale: 'ja',
      m: messages.ja,
    });

    // タブ表示。
    const tabs = screen.getByRole('navigation', { name: messages.ja.project.sectionsLabel });
    expect(within(tabs).getByRole('link', { name: messages.ja.project.overviewTab }).getAttribute('href'))
      .toBe(projectPageHref(projectFixture.id));
    expect(within(tabs).getByRole('link', { name: messages.ja.project.derivativesTab }).getAttribute('href'))
      .toBe(derivativesPageHref(projectFixture.id));
    expect(within(tabs).getByRole('link', { name: messages.ja.project.inspectionTab }).getAttribute('href'))
      .toBe(inspectionPageHref(projectFixture.id));

    // 診断を実行。
    const button = screen.getByRole('button', { name: messages.ja.inspection.runDiagnostics });
    await fireEvent.click(button);

    // 結果表示。
    expect(await screen.findByText(messages.ja.inspection.statusSucceeded)).toBeTruthy();
    expect(screen.getByText(messages.ja.inspection.resultTitle)).toBeTruthy();
    expect(screen.getByText(messages.ja.inspection.ok)).toBeTruthy();
    expect(screen.getByText('注意: 単語が少ない')).toBeTruthy();
    expect(screen.getByText('words.md')).toBeTruthy();
  });

  it('shows an error state when the diagnostics job fails', async () => {
    const jobId = 'job_2';
    server.use(
      http.post('http://hub.test/api/v1/projects/:projectId/diagnostics', () =>
        HttpResponse.json({ jobId, status: 'pending' }, { status: 202 }),
      ),
      http.get('http://hub.test/api/v1/jobs/:jobId', () =>
        HttpResponse.json({
          id: jobId,
          kind: 'diagnostics',
          status: 'failed',
          result: null,
          error: '画像を読み込めませんでした',
          createdAt: '2026-01-01T00:00:00Z',
          finishedAt: '2026-01-01T00:00:01Z',
        }),
      ),
    );

    render(InspectionPage, {
      projectId: projectFixture.id,
      locale: 'ja',
      m: messages.ja,
    });

    await fireEvent.click(screen.getByRole('button', { name: messages.ja.inspection.runDiagnostics }));

    expect(await screen.findByText(messages.ja.inspection.statusFailed)).toBeTruthy();
    expect(screen.getByText(`${messages.ja.inspection.error}: 画像を読み込めませんでした`)).toBeTruthy();
  });

  it('shows an error state when the diagnostics endpoint returns 404', async () => {
    server.use(
      http.post('http://hub.test/api/v1/projects/:projectId/diagnostics', () =>
        HttpResponse.json({ type: 'https://example.test/problems/not-found', title: 'Not Found', status: 404 }, {
          status: 404,
        }),
      ),
    );

    render(InspectionPage, {
      projectId: projectFixture.id,
      locale: 'ja',
      m: messages.ja,
    });

    await fireEvent.click(screen.getByRole('button', { name: messages.ja.inspection.runDiagnostics }));
    expect(await screen.findByText(messages.ja.api.errors['not-found'])).toBeTruthy();
  });
});
