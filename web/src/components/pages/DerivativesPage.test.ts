import { render, screen, within } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import DerivativesPage from './DerivativesPage.svelte';
import { messages } from '../../lib/i18n';
import { derivativesPageHref, projectPageHref } from '../../lib/links';
import { derivativeNodesFixture, projectFixture } from '../../test/fixtures';
import { server } from '../../test/server';

describe('DerivativesPage', () => {
  it('renders the project relationships as a nested tree from the mock API', async () => {
    const requestedPaths: string[] = [];
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId', ({ request }) => {
        requestedPaths.push(new URL(request.url).pathname);
        return HttpResponse.json(projectFixture);
      }),
      http.get('http://hub.test/api/v1/projects/:projectId/derivatives', ({ request }) => {
        requestedPaths.push(new URL(request.url).pathname);
        return HttpResponse.json(derivativeNodesFixture);
      }),
    );

    render(DerivativesPage, {
      projectId: projectFixture.id,
      locale: 'ja',
      m: messages.ja,
    });

    expect(
      await screen.findByRole('heading', {
        name: `${projectFixture.title} — ${messages.ja.derivatives.title}`,
        level: 1,
      }),
    ).toBeTruthy();

    const tree = screen.getByRole('list', { name: messages.ja.derivatives.treeTitle });
    const treeItems = within(tree).getAllByRole('listitem');
    expect(treeItems.length).toBe(5);
    expect(within(tree).getByText('原作・星を渡る')).toBeTruthy();
    expect(within(tree).getByText(projectFixture.title)).toBeTruthy();
    expect(within(tree).getByText('もうひとつの星')).toBeTruthy();
    expect(within(tree).getByText('サンプル小説・その後')).toBeTruthy();
    expect(within(tree).getByText('灯りの向こう側')).toBeTruthy();
    expect(within(tree).getByText('派生元')).toBeTruthy();
    expect(within(tree).getByText('関連する派生作品')).toBeTruthy();
    const currentItem = within(tree).getByRole('link', { name: projectFixture.title }).closest('li');
    const siblingItem = within(tree).getByRole('link', { name: 'もうひとつの星' }).closest('li');
    expect(currentItem?.parentElement).toBe(siblingItem?.parentElement);
    const descendantItem = within(tree).getByRole('link', { name: '灯りの向こう側' }).closest('li');
    expect(descendantItem?.parentElement?.parentElement?.textContent).toContain('サンプル小説・その後');
    expect(within(tree).getByText('条件付きで派生可')).toBeTruthy();
    expect(within(tree).getByText('派生不可')).toBeTruthy();
    expect(requestedPaths).toContain(`/api/v1/projects/${projectFixture.id}/derivatives`);

    const tabs = screen.getByRole('navigation', { name: messages.ja.project.sectionsLabel });
    expect(within(tabs).getByRole('link', { name: messages.ja.project.overviewTab }).getAttribute('href'))
      .toBe(projectPageHref(projectFixture.id));
    expect(within(tabs).getByRole('link', { name: messages.ja.project.derivativesTab }).getAttribute('href'))
      .toBe(derivativesPageHref(projectFixture.id));
  });

  it('uses English labels for derivative metadata', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId', () => HttpResponse.json(projectFixture)),
      http.get('http://hub.test/api/v1/projects/:projectId/derivatives', () =>
        HttpResponse.json(derivativeNodesFixture),
      ),
    );

    render(DerivativesPage, {
      projectId: projectFixture.id,
      locale: 'en',
      m: messages.en,
    });

    expect(await screen.findByText('Sources and derivatives')).toBeTruthy();
    expect(screen.getByText('This project')).toBeTruthy();
    expect(screen.getByText('Allowed with conditions')).toBeTruthy();
  });
});
