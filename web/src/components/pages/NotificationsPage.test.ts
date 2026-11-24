import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import NotificationsPage from './NotificationsPage.svelte';
import { messages } from '../../lib/i18n';
import {
  notificationFixture,
  notificationPageFixture,
  notificationUnreadFixture,
} from '../../test/collaboration-fixtures';
import { server } from '../../test/server';

const listUrl = 'http://hub.test/api/v1/notifications';

function renderPage() {
  render(NotificationsPage, { locale: 'ja', m: messages.ja });
}

describe('NotificationsPage', () => {
  it('lists notifications and sends kind and unread filters using the contract query', async () => {
    let requestedUrl: URL | undefined;
    server.use(
      http.get(listUrl, ({ request }) => {
        requestedUrl = new URL(request.url);
        return HttpResponse.json(notificationPageFixture);
      }),
    );

    renderPage();

    expect(await screen.findByRole('heading', { name: notificationFixture.title, level: 2 })).toBeTruthy();
    expect(screen.getByText(notificationUnreadFixture.title)).toBeTruthy();

    fireEvent.change(screen.getByLabelText('種別で絞り込む'), { target: { value: 'proposal_merged' } });
    fireEvent.click(screen.getByRole('button', { name: '種別で絞り込む' }));

    await waitFor(() => {
      expect(requestedUrl?.searchParams.get('kind')).toBe('proposal_merged');
    });
  });

  it('marks a single notification as read through the contract endpoint', async () => {
    let readBody: unknown;
    let authorization: string | null = null;
    server.use(
      http.get(listUrl, () => HttpResponse.json(notificationPageFixture)),
      http.post(`${listUrl}/read`, async ({ request }) => {
        readBody = await request.json();
        authorization = request.headers.get('Authorization');
        return new HttpResponse(null, { status: 204 });
      }),
    );

    renderPage();

    const item = (await screen.findByText(notificationFixture.title)).closest('li');
    const button = item?.querySelector('button');
    fireEvent.click(button as HTMLButtonElement);

    await waitFor(() => expect(readBody).toEqual({ ids: [notificationFixture.id] }));
    expect(authorization).toBe('Bearer mock-user-token');
    await waitFor(() => expect(screen.getByText('既読にしました。')).toBeTruthy());
  });

  it('marks all notifications as read through the contract endpoint', async () => {
    let readBody: unknown;
    server.use(
      http.get(listUrl, () => HttpResponse.json(notificationPageFixture)),
      http.post(`${listUrl}/read`, async ({ request }) => {
        readBody = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );

    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: 'すべて既読にする' }));

    await waitFor(() => expect(readBody).toEqual({ all: true }));
    await waitFor(() => expect(screen.getByText('すべて既読にしました。')).toBeTruthy());
  });
});
