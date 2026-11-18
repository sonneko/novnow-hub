import { render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { afterEach, describe, expect, it } from 'vitest';
import ReaderPage from './ReaderPage.svelte';
import { messages } from '../../lib/i18n';
import { projectPageHref } from '../../lib/links';
import { clearCachedReadingDocument, cacheReadingDocument, getCachedReadingDocument } from '../../lib/reader-cache';
import { projectFixture, readingDocumentFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('ReaderPage', () => {
  afterEach(async () => {
    await clearCachedReadingDocument(projectFixture.id);
  });

  it('renders the reader-ready blocks returned by the mock API and caches them by ETag', async () => {
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
    expect(await getCachedReadingDocument(projectFixture.id)).toEqual(readingDocumentFixture);
  });

  it('shows the cached document immediately and refreshes it when the ETag changes', async () => {
    const cachedDocument = {
      ...readingDocumentFixture,
      title: '保存済みのサンプル小説',
      etag: 'W/"reading-old"',
    };
    const latestDocument = {
      ...readingDocumentFixture,
      title: '更新されたサンプル小説',
      etag: 'W/"reading-new"',
    };
    await cacheReadingDocument(cachedDocument);

    let releaseResponse: (() => void) | undefined;
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId/reading', async () => {
        await new Promise<void>((resolve) => {
          releaseResponse = resolve;
        });
        return HttpResponse.json(latestDocument);
      }),
    );

    render(ReaderPage, {
      projectId: projectFixture.id,
      m: messages.ja,
    });

    expect(await screen.findByRole('heading', { name: cachedDocument.title, level: 1 })).toBeTruthy();
    expect(screen.getByText(messages.ja.reader.cacheRefreshing)).toBeTruthy();
    await waitFor(() => expect(releaseResponse).toBeDefined());
    releaseResponse?.();

    expect(await screen.findByRole('heading', { name: latestDocument.title, level: 1 })).toBeTruthy();
    expect(await getCachedReadingDocument(projectFixture.id)).toEqual(latestDocument);
    expect(screen.queryByText(messages.ja.reader.cacheRefreshing)).toBeNull();
  });

  it('keeps a cached public document visible on a transient API failure', async () => {
    await cacheReadingDocument(readingDocumentFixture);
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId/reading', () => HttpResponse.error()),
    );

    render(ReaderPage, {
      projectId: projectFixture.id,
      m: messages.ja,
    });

    expect(await screen.findByRole('heading', { name: readingDocumentFixture.title, level: 1 })).toBeTruthy();
    expect(await screen.findByText(messages.ja.reader.cacheOffline)).toBeTruthy();
  });

  it('purges cached content when the public reading endpoint returns 404', async () => {
    await cacheReadingDocument(readingDocumentFixture);
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId/reading', () =>
        HttpResponse.json({ code: 'not_found', title: 'Not Found', status: 404 }, { status: 404 }),
      ),
    );

    render(ReaderPage, {
      projectId: projectFixture.id,
      m: messages.ja,
    });

    expect((await screen.findByRole('alert')).textContent).toContain(
      messages.ja.api.errors['not-found'],
    );
    expect(await getCachedReadingDocument(projectFixture.id)).toBeUndefined();
  });
});
