import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import IssuesPage from './IssuesPage.svelte';
import { messages } from '../../lib/i18n';
import { issueDetailHref } from '../../lib/links';
import { issueFixture, issuePageFixture } from '../../test/collaboration-fixtures';
import { server } from '../../test/server';

describe('IssuesPage', () => {
  it('lists issues and sends state and label filters using the contract query', async () => {
    let requestedUrl: URL | undefined;
    server.use(
      http.get('http://hub.test/api/v1/projects/project-1/issues', ({ request }) => {
        requestedUrl = new URL(request.url);
        return HttpResponse.json(issuePageFixture);
      }),
    );

    render(IssuesPage, { projectId: 'project-1', locale: 'ja', m: messages.ja });
    expect(await screen.findByRole('heading', { name: issueFixture.title, level: 2 })).toBeTruthy();
    fireEvent.change(screen.getByLabelText('状態で絞り込む'), { target: { value: 'open' } });
    fireEvent.input(screen.getByRole('textbox', { name: 'ラベル' }), { target: { value: 'content' } });
    fireEvent.click(screen.getByRole('button', { name: '検索する' }));

    await waitFor(() => {
      expect(requestedUrl?.searchParams.get('state')).toBe('open');
      expect(requestedUrl?.searchParams.get('label')).toBe('content');
    });
    expect(screen.getByRole('link', { name: issueFixture.title }).getAttribute('href')).toBe(
      issueDetailHref('project-1', issueFixture.number),
    );
  });
});
