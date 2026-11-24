import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import ProposalDetailPage from './ProposalDetailPage.svelte';
import { messages } from '../../lib/i18n';
import {
  automergeStateFixture,
  commentPageFixture,
  proposalDiffFixture,
  proposalFixture,
} from '../../test/collaboration-fixtures';
import { server } from '../../test/server';

const proposalUrl = 'http://hub.test/api/v1/projects/project-1/proposals/24';

function useProposalHandlers(onReview: (body: unknown, authorization: string | null) => void, reviewResponse = { ...proposalFixture, state: 'approved', approvalCount: 1 }, automerge = automergeStateFixture) {
  server.use(
    http.get(proposalUrl, () => HttpResponse.json(proposalFixture)),
    http.get(`${proposalUrl}/diff`, () => HttpResponse.json(proposalDiffFixture)),
    http.get(`${proposalUrl}/comments`, () => HttpResponse.json(commentPageFixture)),
    http.get(`${proposalUrl}/automerge`, () => HttpResponse.json(automerge)),
    http.post(`${proposalUrl}/approve`, async ({ request }) => {
      onReview(await request.json(), request.headers.get('Authorization'));
      return HttpResponse.json(reviewResponse);
    }),
  );
}

function renderPage() {
  render(ProposalDetailPage, {
    projectId: 'project-1',
    number: 24,
    locale: 'ja',
    m: messages.ja,
  });
}

describe('ProposalDetailPage', () => {
  it('shows the change set and sends a review through the contract endpoint', async () => {
    let reviewBody: unknown;
    let authorization: string | null = null;
    useProposalHandlers((body, auth) => {
      reviewBody = body;
      authorization = auth;
    });

    renderPage();

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

  it('offers request-changes and dismiss as verdicts of the approve operation', async () => {
    const verdicts: unknown[] = [];
    useProposalHandlers((body) => verdicts.push(body), { ...proposalFixture, state: 'open' });

    renderPage();
    const verdictSelect = (await screen.findByLabelText('判断')) as HTMLSelectElement;
    const options = Array.from(verdictSelect.options).map((option) => option.value);
    expect(options).toEqual(['approve', 'request_changes', 'dismiss']);

    fireEvent.change(verdictSelect, { target: { value: 'request_changes' } });
    fireEvent.click(screen.getByRole('button', { name: 'レビューを送信' }));
    await waitFor(() => expect(verdicts).toHaveLength(1));
    expect(verdicts[0]).toEqual({ verdict: 'request_changes' });

    fireEvent.change(verdictSelect, { target: { value: 'dismiss' } });
    fireEvent.click(screen.getByRole('button', { name: 'レビューを送信' }));
    await waitFor(() => expect(verdicts).toHaveLength(2));
    expect(verdicts[1]).toEqual({ verdict: 'dismiss' });
  });

  it('shows the proposal comment thread from the contract list endpoint', async () => {
    useProposalHandlers(() => {});

    renderPage();

    const thread = await screen.findByRole('region', { name: 'コメント' });
    await waitFor(() => expect(thread.textContent).toContain('第二章の描写は前後の流れと合っていますか。'));
    await waitFor(() => expect(thread.textContent).toContain('次の場面にもつながります。'));
    expect(thread.textContent).toContain('mizu');
    expect(thread.textContent).toContain('14 行目');
  });

  it('shows the auto-merge status and conditions from the contract endpoint', async () => {
    useProposalHandlers(() => {}, undefined, automergeStateFixture);

    renderPage();

    const panel = await screen.findByRole('region', { name: '自動マージ' });
    await waitFor(() => expect(panel.textContent).toContain('予約済み'));
    expect(panel.textContent).toContain('novnow 形式チェック');
    // 予約済みは取消可能で、再予約は不可。
    expect(screen.getByRole('button', { name: '予約を取り消す' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: '自動マージを予約' })).toBeNull();
  });

  it('reserves auto-merge with the required approvals through the contract endpoint', async () => {
    let putBody: unknown;
    let putAuth: string | null = null;
    let putIfMatch: string | null = null;
    server.use(
      http.get(proposalUrl, () => HttpResponse.json(proposalFixture)),
      http.get(`${proposalUrl}/diff`, () => HttpResponse.json(proposalDiffFixture)),
      http.get(`${proposalUrl}/comments`, () => HttpResponse.json(commentPageFixture)),
      http.get(`${proposalUrl}/automerge`, () => HttpResponse.json({ ...automergeStateFixture, state: 'none' })),
      http.put(`${proposalUrl}/automerge`, async ({ request }) => {
        putBody = await request.json();
        putAuth = request.headers.get('Authorization');
        putIfMatch = request.headers.get('If-Match');
        return HttpResponse.json({ ...automergeStateFixture, state: 'reserved' });
      }),
    );

    renderPage();
    const input = (await screen.findByLabelText('承認数')) as HTMLInputElement;
    fireEvent.input(input, { target: { value: '2' } });
    fireEvent.click(screen.getByRole('button', { name: '自動マージを予約' }));

    await waitFor(() => expect(putBody).toEqual({ requiredApprovals: 2 }));
    expect(putAuth).toBe('Bearer mock-user-token');
    expect(putIfMatch).toBe(automergeStateFixture.etag);
    expect(await screen.findByText('予約済み')).toBeTruthy();
  });

  it('cancels a reserved auto-merge through the contract endpoint', async () => {
    let deleted = false;
    server.use(
      http.get(proposalUrl, () => HttpResponse.json(proposalFixture)),
      http.get(`${proposalUrl}/diff`, () => HttpResponse.json(proposalDiffFixture)),
      http.get(`${proposalUrl}/comments`, () => HttpResponse.json(commentPageFixture)),
      http.get(`${proposalUrl}/automerge`, () => HttpResponse.json(automergeStateFixture)),
      http.delete(`${proposalUrl}/automerge`, () => {
        deleted = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );

    renderPage();
    const cancelButton = await screen.findByRole('button', { name: '予約を取り消す' });
    fireEvent.click(cancelButton);

    await waitFor(() => expect(deleted).toBe(true));
    expect(await screen.findByText(/予約を取り消しました/, {}, { timeout: 3000 })).toBeTruthy();
  });
});
