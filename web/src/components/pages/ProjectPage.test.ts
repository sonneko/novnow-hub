import { fireEvent, render, screen } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import ProjectPage from './ProjectPage.svelte';
import { messages } from '../../lib/i18n';
import { readerPageHref } from '../../lib/links';
import {
  fileContentFixture,
  projectFixture,
  setupInfoFixture,
  treeFixture,
} from '../../test/fixtures';
import { server } from '../../test/server';

describe('ProjectPage', () => {
  it('shows project metadata, file structure, setup URL, and reader entry point', async () => {
    let fileRequest: URL | undefined;
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId', () =>
        HttpResponse.json(projectFixture),
      ),
      http.get('http://hub.test/api/v1/projects/:projectId/tree', () =>
        HttpResponse.json(treeFixture),
      ),
      http.get('http://hub.test/api/v1/projects/:projectId/setup', () =>
        HttpResponse.json(setupInfoFixture),
      ),
      http.get('http://hub.test/api/v1/projects/:projectId/file', ({ request }) => {
        fileRequest = new URL(request.url);
        return HttpResponse.json(fileContentFixture);
      }),
    );

    render(ProjectPage, {
      projectId: projectFixture.id,
      locale: 'ja',
      m: messages.ja,
    });

    expect(
      await screen.findByRole('heading', {
        name: projectFixture.title,
        level: 1,
      }),
    ).toBeTruthy();
    expect(screen.getByText(projectFixture.description!)).toBeTruthy();
    expect(screen.getByText('sora')).toBeTruthy();
    expect(screen.getByText('cc-by')).toBeTruthy();
    expect(screen.getByText('1', { selector: 'dd' })).toBeTruthy();
    expect(screen.getByText('novnow/01-opening.md')).toBeTruthy();
    expect(screen.getByRole('link', { name: '読者ビューで読む' }).getAttribute('href')).toBe(
      readerPageHref(projectFixture.id),
    );
    expect(screen.getByRole('link', { name: setupInfoFixture.acquisition.url }).getAttribute('href')).toBe(
      setupInfoFixture.acquisition.url,
    );

    fireEvent.click(screen.getByRole('button', { name: '内容を見る' }));
    expect(await screen.findByText(/これは作品ファイルのプレビューです/)).toBeTruthy();
    expect(fileRequest?.searchParams.get('path')).toBe(fileContentFixture.path);
  });
});
