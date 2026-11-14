import { render, screen } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import ProjectPage from './ProjectPage.svelte';
import { messages } from '../../lib/i18n';
import { readerPageHref } from '../../lib/links';
import { projectFixture, treeFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('ProjectPage', () => {
  it('loads project details and its contents through the contract endpoints', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId', () =>
        HttpResponse.json(projectFixture),
      ),
      http.get('http://hub.test/api/v1/projects/:projectId/tree', () =>
        HttpResponse.json(treeFixture),
      ),
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
    expect(screen.getByText('novnow/01-opening.md')).toBeTruthy();
    expect(screen.getByRole('link', { name: '読者ビューで読む' }).getAttribute('href')).toBe(
      readerPageHref(projectFixture.id),
    );
  });
});
