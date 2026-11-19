// @vitest-environment node
import { afterEach, describe, expect, it } from 'vitest';
import { createChatApi } from '../../server/chat-api.mjs';
import { SqliteChatPort } from '../../server/chat-port.mjs';

let chatPort: SqliteChatPort | undefined;

afterEach(() => {
  chatPort?.close();
  chatPort = undefined;
});

function makeApi() {
  chatPort = new SqliteChatPort({ databasePath: ':memory:' });
  return createChatApi({
    chatPort,
    resolveActor: (request) => {
      const token = request.headers.get('authorization');
      if (token === 'Bearer sora-token') return 'sora';
      if (token === 'Bearer mizu-token') return 'mizu';
      if (token === 'Bearer owner-token') return 'owner';
      return undefined;
    },
    isProjectOwner: (_request, _projectId, actorHandle) => actorHandle === 'owner',
  });
}

function apiRequest(method: string, path: string, body?: unknown, token?: string) {
  return new Request(`http://hub.test${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { 'content-type': 'application/json' }),
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
}

describe('Chat API contract routes', () => {
  it('uses the existing channels/messages routes for room and message lifecycle', async () => {
    const api = makeApi();
    const root = '/projects/project-1/chat/channels';
    const anonymousList = await api(apiRequest('GET', root));
    expect(anonymousList.status).toBe(200);
    expect(await anonymousList.json()).toEqual([]);

    const unauthenticatedCreate = await api(apiRequest('POST', root, { name: '制作' }));
    expect(unauthenticatedCreate.status).toBe(401);
    expect(unauthenticatedCreate.headers.get('content-type')).toContain('application/problem+json');

    const created = await api(apiRequest('POST', root, { name: '制作', description: '相談用' }, 'sora-token'));
    expect(created.status).toBe(201);
    const room = await created.json();
    expect(room).toMatchObject({ name: '制作', description: '相談用', unreadCount: 0 });

    const messagesPath = `${root}/${room.id}/messages`;
    const posted = await api(apiRequest('POST', messagesPath, { body: '見てください @mizu' }, 'sora-token'));
    expect(posted.status).toBe(201);
    const message = await posted.json();
    expect(message).toMatchObject({ authorHandle: 'sora', body: '見てください @mizu', mentions: ['mizu'], deleted: false });

    const edited = await api(apiRequest('PATCH', `${messagesPath}/${message.id}`, { body: '確認しました @mizu' }, 'sora-token'));
    expect(edited.status).toBe(200);
    expect(await edited.json()).toMatchObject({ body: '確認しました @mizu', editedAt: expect.any(String) });

    const forbiddenDelete = await api(apiRequest('DELETE', `${messagesPath}/${message.id}`, undefined, 'mizu-token'));
    expect(forbiddenDelete.status).toBe(403);
    const deleted = await api(apiRequest('DELETE', `${messagesPath}/${message.id}`, undefined, 'owner-token'));
    expect(deleted.status).toBe(204);

    const history = await api(apiRequest('GET', messagesPath));
    expect(await history.json()).toMatchObject({
      items: [{ id: message.id, body: '［削除されました］', deleted: true }],
      nextCursor: null,
    });
  });

  it('returns contract-shaped validation and not-found problems', async () => {
    const api = makeApi();
    const invalid = await api(apiRequest('POST', '/projects/project-1/chat/channels', { name: '' }, 'sora-token'));
    expect(invalid.status).toBe(400);
    expect(await invalid.json()).toMatchObject({ status: 400, code: 'validation_failed', traceId: expect.any(String) });

    const missing = await api(apiRequest('GET', '/projects/project-1/chat/channels/missing/messages'));
    expect(missing.status).toBe(404);
    expect(await missing.json()).toMatchObject({ status: 404, code: 'not_found' });
  });
});
