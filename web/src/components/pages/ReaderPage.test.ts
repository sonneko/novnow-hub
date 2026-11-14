import { render, screen } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import ReaderPage from './ReaderPage.svelte';
import { messages } from '../../lib/i18n';
import { projectPageHref } from '../../lib/links';
import { projectFixture, readingDocumentFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('ReaderPage', () => {
  it('renders the reading document blocks returned by the mock API', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId/reading', () =>
        HttpResponse.json(readingDocumentFixture),
      ),
    );

    render(ReaderPage, {
      projectId: projectFixture.id,
      m: messages.ja,
    });

    expect(
      await screen.findByRole('heading', {
        name: readingDocumentFixture.title,
        level: 1,
      }),
    ).toBeTruthy();
    expect(screen.getByRole('heading', { name: '第一章', level: 2 })).toBeTruthy();
    expect(screen.getByText('API から取得した本文です。')).toBeTruthy();
    expect(screen.getByText('ミナ')).toBeTruthy();
    expect(screen.getByRole('separator', { name: '場面の区切り' })).toBeTruthy();
    expect(
      screen
        .getByRole('link', { name: 'プロジェクトへ戻る' })
        .getAttribute('href'),
    ).toBe(projectPageHref(projectFixture.id));
  });
});
