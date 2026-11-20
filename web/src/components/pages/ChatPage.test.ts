import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import ChatPage from './ChatPage.svelte';
import { messages } from '../../lib/i18n';
import type { ChatChannel, ChatMessage } from '../../lib/api/types';
import { server } from '../../test/server';

const roomFixture: ChatChannel = {
  id: 'room-general',
  name: '全体',
  description: 'プロジェクト全体',
  unreadCount: 0,
  createdAt: '2026-11-15T10:00:00Z',
};
const socialRoomFixture: ChatChannel = {
  id: 'room-social',
  name: '雑談',
  description: '気軽な会話',
  unreadCount: 3,
  createdAt: '2026-11-15T10:01:00Z',
};
const messageFixture: ChatMessage = {
  id: 'message-1',
  channelId: roomFixture.id,
  authorHandle: 'sora',
  body: '最初のメッセージ',
  mentions: [],
  editedAt: null,
  deleted: false,
  createdAt: '2026-11-15T10:02:00Z',
};

function registerChatHandlers() {
  let rooms: ChatChannel[] = [{ ...roomFixture }, { ...socialRoomFixture }];
  const messagesByRoom = new Map<string, ChatMessage[]>([[roomFixture.id, [{ ...messageFixture }]]]);
  let id = 1;
  let lastSentBody = '';

  server.use(
    http.get('http://hub.test/api/v1/projects/:projectId/chat/channels', () =>
      HttpResponse.json(rooms),
    ),
    http.post('http://hub.test/api/v1/projects/:projectId/chat/channels', async ({ request }) => {
      const body = await request.json() as { name: string; description?: string };
      const room: ChatChannel = {
        id: `room-new-${id++}`,
        name: body.name,
        description: body.description ?? '',
        unreadCount: 0,
        createdAt: '2026-11-15T10:03:00Z',
      };
      rooms = [...rooms, room];
      messagesByRoom.set(room.id, []);
      return HttpResponse.json(room, { status: 201 });
    }),
    http.get('http://hub.test/api/v1/projects/:projectId/chat/channels/:channelId/messages', ({ params }) =>
      HttpResponse.json({ items: messagesByRoom.get(String(params.channelId)) ?? [], nextCursor: null }),
    ),
    http.post('http://hub.test/api/v1/projects/:projectId/chat/channels/:channelId/messages', async ({ request, params }) => {
      const body = await request.json() as { body: string };
      lastSentBody = body.body;
      const message: ChatMessage = {
        id: `message-${id++}`,
        channelId: String(params.channelId),
        authorHandle: 'sora',
        body: body.body,
        mentions: [...body.body.matchAll(/@([\w-]+)/g)].map((match) => match[1]),
        editedAt: null,
        deleted: false,
        createdAt: new Date(Date.now() + id).toISOString(),
      };
      messagesByRoom.set(message.channelId, [...(messagesByRoom.get(message.channelId) ?? []), message]);
      return HttpResponse.json(message, { status: 201 });
    }),
    http.patch('http://hub.test/api/v1/projects/:projectId/chat/channels/:channelId/messages/:messageId', async ({ request, params }) => {
      const body = await request.json() as { body: string };
      const channelId = String(params.channelId);
      const existing = messagesByRoom.get(channelId) ?? [];
      const updated = existing.map((message) => message.id === params.messageId
        ? { ...message, body: body.body, mentions: [...body.body.matchAll(/@([\w-]+)/g)].map((match) => match[1]), editedAt: new Date().toISOString() }
        : message);
      messagesByRoom.set(channelId, updated);
      return HttpResponse.json(updated.find((message) => message.id === params.messageId));
    }),
    http.delete('http://hub.test/api/v1/projects/:projectId/chat/channels/:channelId/messages/:messageId', ({ params }) => {
      const channelId = String(params.channelId);
      const existing = messagesByRoom.get(channelId) ?? [];
      messagesByRoom.set(channelId, existing.map((message) => message.id === params.messageId
        ? { ...message, body: '［削除されました］', mentions: [], deleted: true }
        : message));
      return new HttpResponse(null, { status: 204 });
    }),
  );
  return { getLastSentBody: () => lastSentBody };
}

describe('ChatPage', () => {
  it('shows unread room badges, creates a room, sends and polls messages through the chat API', async () => {
    const mock = registerChatHandlers();
    render(ChatPage, { projectId: 'project-1', locale: 'ja', m: messages.ja });

    expect(await screen.findByRole('heading', { name: '全体', level: 2 })).toBeTruthy();
    expect(screen.getByLabelText('3 件の未読メッセージ')).toBeTruthy();
    expect(screen.getByText(/5秒ごと/)).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'ルームを作成' }));
    fireEvent.input(screen.getByLabelText('ルーム名'), { target: { value: '読書会' } });
    fireEvent.input(screen.getByLabelText('説明（任意）'), { target: { value: '感想を話す' } });
    fireEvent.click(screen.getByRole('button', { name: 'ルームを作成' }));
    expect(await screen.findByRole('heading', { name: '読書会', level: 2 })).toBeTruthy();

    fireEvent.input(screen.getByLabelText('メッセージ'), { target: { value: 'こんにちは @mizu' } });
    fireEvent.click(screen.getByRole('button', { name: '送信' }));
    expect(await screen.findByText('こんにちは @mizu')).toBeTruthy();
    await waitFor(() => expect(mock.getLastSentBody()).toBe('こんにちは @mizu'));
  });

  it('lets the author edit a message and keeps a tombstone after deletion', async () => {
    registerChatHandlers();
    render(ChatPage, { projectId: 'project-1', locale: 'ja', m: messages.ja });
    expect(await screen.findByText('最初のメッセージ')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: '編集' }));
    fireEvent.input(screen.getByLabelText('編集'), { target: { value: '編集したメッセージ' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    expect(await screen.findByText('編集したメッセージ')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: '削除' }));
    expect(await screen.findByText('［削除されました］')).toBeTruthy();
  });
});
