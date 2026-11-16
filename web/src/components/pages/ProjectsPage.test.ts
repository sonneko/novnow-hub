import { render, screen } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import ProjectsPage from './ProjectsPage.svelte';
import { messages } from '../../lib/i18n';
import { projectPageHref, readerPageHref } from '../../lib/links';
import { projectFixture, projectPageFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('ProjectsPage', () => {
  it('renders projects returned by the mock API', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects', () =>
        HttpResponse.json(projectPageFixture),
      ),
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
});
