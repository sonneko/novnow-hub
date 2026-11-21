import { render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import IssueDetailPage from './IssueDetailPage.svelte';
import { messages } from '../../lib/i18n';
import { proposalDetailHref } from '../../lib/links';
import { commentPageFixture, issueFixture } from '../../test/collaboration-fixtures';
import { server } from '../../test/server';

describe('IssueDetailPage', () => {
  it('shows task details and a related proposal link from the contract response', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects/project-1/issues/7', () =>
        HttpResponse.json(issueFixture),
      ),
      http.get('http://hub.test/api/v1/projects/project-1/issues/7/comments', () =>
        HttpResponse.json(commentPageFixture),
      ),
    );

    render(IssueDetailPage, { projectId: 'project-1', number: 7, locale: 'ja', m: messages.ja });

    expect(await screen.findByRole('heading', { name: issueFixture.title, level: 1 })).toBeTruthy();
    expect(screen.getByText(issueFixture.body)).toBeTruthy();
    expect(screen.getByText('投稿者: mizu')).toBeTruthy();
    expect(screen.getByText('担当者: sora')).toBeTruthy();
    expect(screen.getByRole('link', { name: '#24' }).getAttribute('href')).toBe(
      proposalDetailHref('project-1', 24),
    );
  });

  it('shows the task comment thread from the contract list endpoint', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects/project-1/issues/7', () =>
        HttpResponse.json(issueFixture),
      ),
      http.get('http://hub.test/api/v1/projects/project-1/issues/7/comments', () =>
        HttpResponse.json(commentPageFixture),
      ),
    );

    render(IssueDetailPage, { projectId: 'project-1', number: 7, locale: 'ja', m: messages.ja });

    const thread = await screen.findByRole('region', { name: 'コメント' });
    await waitFor(() => expect(thread.textContent).toContain('第二章の描写は前後の流れと合っていますか。'));
    expect(thread.textContent).toContain('sora');
  });
});
