import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import ProposalDetailPage from './ProposalDetailPage.svelte';
import { messages } from '../../lib/i18n';
import { proposalFixture, proposalDiffFixture } from '../../test/collaboration-fixtures';
import { server } from '../../test/server';

describe('ProposalDetailPage', () => {
  it('shows the change set and sends a review through the contract endpoint', async () => {
    let reviewBody: unknown;
    let authorization: string | null = null;
    server.use(
      http.get('http://hub.test/api/v1/projects/project-1/proposals/24', () =>
        HttpResponse.json(proposalFixture),
      ),
      http.get('http://hub.test/api/v1/projects/project-1/proposals/24/diff', () =>
        HttpResponse.json(proposalDiffFixture),
      ),
      http.post('http://hub.test/api/v1/projects/project-1/proposals/24/approve', async ({ request }) => {
        reviewBody = await request.json();
        authorization = request.headers.get('Authorization');
        return HttpResponse.json({ ...proposalFixture, state: 'approved', approvalCount: 1 });
      }),
    );

    render(ProposalDetailPage, {
      projectId: 'project-1',
      number: 24,
      locale: 'ja',
      m: messages.ja,
    });

    expect(await screen.findByRole('heading', { name: proposalFixture.title, level: 1 })).toBeTruthy();
    expect(screen.getByText('novnow/02-chapter.md')).toBeTruthy();
    expect(screen.getByText(/新しい一文/)).toBeTruthy();
    expect(screen.getByRole('region', { name: 'レビュー' }).textContent).toContain('0 / 1');

    fireEvent.input(screen.getByLabelText('コメント（任意）'), {
      target: { value: '変更の方向性に賛成です。' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'レビューを送信' }));

    expect(await screen.findByText('承認済み')).toBeTruthy();
    await waitFor(() => expect(reviewBody).toEqual({ verdict: 'approve', body: '変更の方向性に賛成です。' }));
    expect(authorization).toBe('Bearer mock-user-token');
  });
});
