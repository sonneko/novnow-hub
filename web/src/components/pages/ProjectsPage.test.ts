import { fireEvent, render, screen } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import ProjectsPage from './ProjectsPage.svelte';
import { messages } from '../../lib/i18n';
import { projectPageHref, readerPageHref } from '../../lib/links';
import type { Project, ProjectPage } from '../../lib/api/types';
import { projectFixture, projectPageFixture } from '../../test/fixtures';
import { server } from '../../test/server';

const secondProject = {
  ...projectFixture,
  id: 'project-2',
  slug: 'another-story',
  title: 'もうひとつの物語',
  license: 'cc0',
  tags: ['poetry'],
} satisfies Project;

describe('ProjectsPage', () => {
  it('renders projects returned by the mock API', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects', () => HttpResponse.json(projectPageFixture)),
    );

    render(ProjectsPage, { locale: 'ja', m: messages.ja });

    expect(
      await screen.findByRole('heading', {
        name: projectFixture.title,
        level: 2,
      }),
    ).toBeTruthy();
    expect(screen.getByText('作者: sora')).toBeTruthy();
    expect(screen.getByRole('link', { name: projectFixture.title }).getAttribute('href')).toBe(
      projectPageHref(projectFixture.id),
    );
    expect(
      screen.getByRole('link', { name: '読者ビューで読む' }).getAttribute('href'),
    ).toBe(readerPageHref(projectFixture.id));
  });

  it('uses the contract search endpoint with keyword and filters', async () => {
    let searchUrl: URL | undefined;
    server.use(
      http.get('http://hub.test/api/v1/projects', () =>
        HttpResponse.json({ items: [], nextCursor: null } satisfies ProjectPage),
      ),
      http.get('http://hub.test/api/v1/search/projects', ({ request }) => {
        searchUrl = new URL(request.url);
        return HttpResponse.json(projectPageFixture);
      }),
    );

    render(ProjectsPage, { locale: 'ja', m: messages.ja });
    fireEvent.input(screen.getByLabelText('キーワード'), { target: { value: 'サンプル' } });
    fireEvent.change(screen.getByLabelText('ライセンス'), { target: { value: 'cc-by' } });
    fireEvent.input(screen.getByLabelText('タグ'), { target: { value: 'fantasy' } });
    fireEvent.click(screen.getByRole('button', { name: '検索する' }));

    expect(await screen.findByRole('heading', { name: projectFixture.title, level: 2 })).toBeTruthy();
    expect(searchUrl?.searchParams.get('q')).toBe('サンプル');
    expect(searchUrl?.searchParams.get('license')).toBe('cc-by');
    expect(searchUrl?.searchParams.get('tag')).toBe('fantasy');
    expect(searchUrl?.searchParams.get('sort')).toBe('relevance');
  });

  it('applies filters to the in-memory index when no keyword is entered', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects', () =>
        HttpResponse.json({ items: [projectFixture, secondProject], nextCursor: null }),
      ),
    );

    render(ProjectsPage, { locale: 'ja', m: messages.ja });
    expect(await screen.findByRole('heading', { name: projectFixture.title, level: 2 })).toBeTruthy();
    fireEvent.change(screen.getByLabelText('ライセンス'), { target: { value: 'cc0' } });
    fireEvent.click(screen.getByRole('button', { name: '検索する' }));

    expect(await screen.findByRole('heading', { name: secondProject.title, level: 2 })).toBeTruthy();
    expect(screen.queryByRole('heading', { name: projectFixture.title, level: 2 })).toBeNull();
  });

  it('loads cursor pages and deduplicates projects in the in-memory index', async () => {
    const cursors: Array<string | null> = [];
    let calls = 0;
    server.use(
      http.get('http://hub.test/api/v1/projects', ({ request }) => {
        cursors.push(new URL(request.url).searchParams.get('cursor'));
        calls += 1;
        return HttpResponse.json(
          calls === 1
            ? { items: [projectFixture], nextCursor: 'page-two' }
            : { items: [projectFixture, secondProject], nextCursor: null },
        );
      }),
    );

    render(ProjectsPage, { locale: 'ja', m: messages.ja });
    expect(await screen.findByRole('heading', { name: projectFixture.title, level: 2 })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '次のページを読み込む' }));

    expect(await screen.findByRole('heading', { name: secondProject.title, level: 2 })).toBeTruthy();
    expect(cursors).toEqual([null, 'page-two']);
    expect(screen.getAllByRole('heading', { name: projectFixture.title, level: 2 })).toHaveLength(1);
  });
});
