// @vitest-environment node
import { EventEmitter } from 'node:events';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { ChatPortError, SqliteChatPort } from '../../server/chat-port.mjs';

let store: SqliteChatPort | undefined;

function makeStore(options: ConstructorParameters<typeof SqliteChatPort>[0] = {}) {
  store = new SqliteChatPort({ databasePath: ':memory:', ...options });
  return store;
}

afterEach(() => {
  store?.close();
  store = undefined;
});

describe('SQLite ChatPort', () => {
  it('creates project-scoped rooms and counts unread messages until the reader opens them', () => {
    const port = makeStore();
    const room = port.createRoom('project-1', { name: '雑談', description: '気軽に話す' });
    port.createRoom('project-2', { name: '雑談' });
    port.sendMessage('project-1', room.id, { authorHandle: 'mizu', body: 'こんにちは @sora' });

    expect(port.listRooms('project-1', { actorHandle: 'sora' })).toMatchObject([
      { id: room.id, name: '雑談', description: '気軽に話す', unreadCount: 1 },
    ]);
    expect(port.listRooms('project-2', { actorHandle: 'sora' })).toHaveLength(1);
    expect(port.listMessages('project-1', room.id, { actorHandle: 'sora' }).items[0].body)
      .toBe('こんにちは @sora');
    expect(port.listRooms('project-1', { actorHandle: 'sora' })[0].unreadCount).toBe(0);
  });

  it('extracts unique mentions and emits durable mention-notification events', () => {
    const eventBus = new EventEmitter();
    const received: unknown[] = [];
    eventBus.on('notification', (event) => received.push(event));
    const port = makeStore({ eventBus });
    const room = port.createRoom('project-1', { name: '全体' });

    const message = port.sendMessage('project-1', room.id, {
      authorHandle: 'mizu',
      body: '確認お願いします @sora！もう一度 @SORA。mail@example.test',
    });

    expect(message.mentions).toEqual(['sora']);
    expect(received).toMatchObject([
      { kind: 'mention', recipientHandle: 'sora', projectId: 'project-1', channelId: room.id, messageId: message.id },
    ]);
    expect(port.listPendingNotificationEvents()).toEqual([]);

    eventBus.removeAllListeners();
    const pendingRoom = port.createRoom('project-1', { name: '制作' });
    port.sendMessage('project-1', pendingRoom.id, { authorHandle: 'mizu', body: '@sora 見てもらえますか' });
    expect(port.listPendingNotificationEvents()).toHaveLength(1);
  });

  it('edits with a revision and soft-deletes while preserving a tombstone in the timeline', () => {
    const port = makeStore();
    const room = port.createRoom('project-1', { name: '制作' });
    const message = port.sendMessage('project-1', room.id, { authorHandle: 'sora', body: '初稿です' });

    expect(() => port.editMessage('project-1', room.id, message.id, {
      actorHandle: 'mizu', body: '第三者からの編集',
    })).toThrow(ChatPortError);
    const edited = port.editMessage('project-1', room.id, message.id, {
      actorHandle: 'sora', body: '修正版です @mizu',
    });
    expect(edited).toMatchObject({ body: '修正版です @mizu', mentions: ['mizu'], editedAt: expect.any(String) });
    expect(port.database.prepare('SELECT body FROM chat_message_revisions WHERE message_id = ?').get(message.id))
      .toEqual({ body: '初稿です' });

    port.deleteMessage('project-1', room.id, message.id, { actorHandle: 'sora' });
    expect(port.listMessages('project-1', room.id).items[0]).toMatchObject({
      id: message.id,
      body: '［削除されました］',
      mentions: [],
      deleted: true,
    });

    const otherAuthorMessage = port.sendMessage('project-1', room.id, {
      authorHandle: 'mizu', body: '所有者なら削除できます',
    });
    port.deleteMessage('project-1', room.id, otherAuthorMessage.id, {
      actorHandle: 'sora', isProjectOwner: true,
    });
    expect(port.listMessages('project-1', room.id).items[1]).toMatchObject({
      id: otherAuthorMessage.id, deleted: true,
    });
  });

  it('returns recent messages in chronological order with an older-page cursor', () => {
    const port = makeStore({
      clock: () => '2026-11-16T12:00:00.000Z',
      idFactory: (() => {
        let id = 0;
        return () => `message-${String(++id).padStart(4, '0')}`;
      })(),
    });
    const room = port.createRoom('project-1', { name: '全体' });
    for (const body of ['one', 'two', 'three']) {
      port.sendMessage('project-1', room.id, { authorHandle: 'sora', body });
    }

    const latest = port.listMessages('project-1', room.id, { limit: 2 });
    expect(latest.items.map((message) => message.body)).toEqual(['two', 'three']);
    expect(latest.nextCursor).toBeTruthy();
    const older = port.listMessages('project-1', room.id, { limit: 2, cursor: latest.nextCursor! });
    expect(older.items.map((message) => message.body)).toEqual(['one']);
    expect(older.nextCursor).toBeNull();
  });

  it('seeds a repeatable local preview conversation with a visible unread mention', () => {
    const port = makeStore();
    port.seedDemoProject('chat-demo');
    port.seedDemoProject('chat-demo');

    expect(port.listRooms('chat-demo').map((room) => room.name)).toEqual(['全体', '制作', '雑談']);
    expect(port.listRooms('chat-demo', { actorHandle: 'sora' }).map((room) => room.unreadCount)).toEqual([0, 0, 1]);
    expect(port.listPendingNotificationEvents()).toMatchObject([
      { kind: 'mention', recipientHandle: 'sora', projectId: 'chat-demo' },
    ]);
  });

  it('persists rooms and messages in the hub-owned SQLite file', () => {
    const directory = mkdtempSync(join(tmpdir(), 'novnow-chat-'));
    const databasePath = join(directory, 'chat.sqlite');
    const first = new SqliteChatPort({ databasePath });
    const room = first.createRoom('project-1', { name: '保管されるルーム' });
    first.sendMessage('project-1', room.id, { authorHandle: 'sora', body: '再起動後も読めます' });
    first.close();
    store = undefined;

    const reopened = new SqliteChatPort({ databasePath });
    store = reopened;
    expect(reopened.listRooms('project-1')[0].name).toBe('保管されるルーム');
    expect(reopened.listMessages('project-1', room.id).items[0].body).toBe('再起動後も読めます');
    reopened.close();
    store = undefined;
    rmSync(directory, { recursive: true, force: true });
  });
});
