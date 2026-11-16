import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import ProposalsPage from './ProposalsPage.svelte';
import { messages } from '../../lib/i18n';
import { proposalDetailHref } from '../../lib/links';
import { proposalFixture, proposalPageFixture } from '../../test/collaboration-fixtures';
import { server } from '../../test/server';

describe('ProposalsPage', () => {
  it('lists proposals from the contract endpoint and applies status filters', async () => {
    let requestedState: string | null = null;
    server.use(
      http.get('http://hub.test/api/v1/projects/project-1/proposals', ({ request }) => {
        requestedState = new URL(request.url).searchParams.get('state');
        return HttpResponse.json(proposalPageFixture);
      }),
    );

    render(ProposalsPage, { projectId: 'project-1', locale: 'ja', m: messages.ja });
    expect(await screen.findByRole('heading', { name: proposalFixture.title, level: 2 })).toBeTruthy();
    fireEvent.change(screen.getByLabelText('状態で絞り込む'), { target: { value: 'open' } });
    fireEvent.click(screen.getByRole('button', { name: '検索する' }));

    expect(await screen.findByRole('link', { name: proposalFixture.title })).toBeTruthy();
    await waitFor(() => expect(requestedState).toBe('open'));
    expect(screen.getByRole('link', { name: proposalFixture.title }).getAttribute('href')).toBe(
      proposalDetailHref('project-1', proposalFixture.number),
    );
  });
});
