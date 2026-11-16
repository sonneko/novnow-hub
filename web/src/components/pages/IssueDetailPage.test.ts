import { render, screen } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import IssueDetailPage from './IssueDetailPage.svelte';
import { messages } from '../../lib/i18n';
import { proposalDetailHref } from '../../lib/links';
import { issueFixture } from '../../test/collaboration-fixtures';
import { server } from '../../test/server';

describe('IssueDetailPage', () => {
  it('shows task details and a related proposal link from the contract response', async () => {
    server.use(
      http.get('http://hub.test/api/v1/projects/project-1/issues/7', () =>
        HttpResponse.json(issueFixture),
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
});
