import { fireEvent, render, screen, within } from '@testing-library/svelte';
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
    // 孫（灯りの向こう側）は初期表示しないため、4 ノード。
    expect(treeItems.length).toBe(4);
    expect(within(tree).getByText('原作・星を渡る')).toBeTruthy();
    expect(within(tree).getByText(projectFixture.title)).toBeTruthy();
    expect(within(tree).getByText('もうひとつの星')).toBeTruthy();
    expect(within(tree).getByText('サンプル小説・その後')).toBeTruthy();
    expect(within(tree).getByText('派生元')).toBeTruthy();
    expect(within(tree).getByText('関連する派生作品')).toBeTruthy();
    const currentItem = within(tree).getByRole('link', { name: projectFixture.title }).closest('li');
    const siblingItem = within(tree).getByRole('link', { name: 'もうひとつの星' }).closest('li');
    expect(currentItem?.parentElement).toBe(siblingItem?.parentElement);
    expect(within(tree).getByText('条件付きで派生可')).toBeTruthy();
    expect(within(tree).getByText('派生不可')).toBeTruthy();
    expect(requestedPaths).toContain(`/api/v1/projects/${projectFixture.id}/derivatives`);

    const tabs = screen.getByRole('navigation', { name: messages.ja.project.sectionsLabel });
    expect(within(tabs).getByRole('link', { name: messages.ja.project.overviewTab }).getAttribute('href'))
      .toBe(projectPageHref(projectFixture.id));
    expect(within(tabs).getByRole('link', { name: messages.ja.project.derivativesTab }).getAttribute('href'))
      .toBe(derivativesPageHref(projectFixture.id));
  });

  it('expands later generations lazily', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId', () => HttpResponse.json(projectFixture)),
      http.get('http://hub.test/api/v1/projects/:projectId/derivatives', () =>
        HttpResponse.json(derivativeNodesFixture),
      ),
    );

    render(DerivativesPage, {
      projectId: projectFixture.id,
      locale: 'ja',
      m: messages.ja,
    });

    const tree = await screen.findByRole('list', { name: messages.ja.derivatives.treeTitle });
    // 孫は初期表示されない。
    expect(within(tree).queryByText('灯りの向こう側')).toBeNull();

    // 孫を持つノード（サンプル小説・その後）の展開ボタンを確認。
    const expandButton = within(tree).getByRole('button', {
      name: new RegExp(messages.ja.derivatives.expand),
    });
    fireEvent.click(expandButton);

    // 展開すると孫が現れる。
    expect(within(tree).getByText('灯りの向こう側')).toBeTruthy();
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

  it('creates a derivative via the POST derive endpoint', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects/:projectId', () => HttpResponse.json(projectFixture)),
      http.get('http://hub.test/api/v1/projects/:projectId/derivatives', () =>
        HttpResponse.json(derivativeNodesFixture),
      ),
      http.post('http://hub.test/api/v1/projects/:projectId/derive', async ({ request }) => {
        const parsed = (await request.json()) as { title?: string };
        expect(parsed.title).toBe('新派生');
        return HttpResponse.json({ jobId: 'job-123', status: 'pending' }, { status: 202 });
      }),
    );

    render(DerivativesPage, {
      projectId: projectFixture.id,
      locale: 'ja',
      m: messages.ja,
    });

    await screen.findByRole('list', { name: messages.ja.derivatives.treeTitle });

    // 派生作成の導線を開く。
    const createButton = screen.getByRole('button', { name: messages.ja.derivatives.createTitle });
    fireEvent.click(createButton);

    // 題名を入力して送信。
    const titleInput = screen.getByRole('textbox', { name: messages.ja.derivatives.titleLabel });
    fireEvent.input(titleInput, { target: { value: '新派生' } });
    const form = titleInput.closest('form') as HTMLElement;
    fireEvent.click(within(form).getByRole('button', { name: messages.ja.derivatives.submit }));

    // 202 の受理結果（ジョブ ID）が表示される。
    expect(await screen.findByRole('status')).toBeTruthy();
    expect(screen.getByRole('status')?.textContent).toContain('job-123');
  });
});
