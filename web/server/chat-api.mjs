import { createServer } from 'node:http';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { ChatPortError, SqliteChatPort } from './chat-port.mjs';

const here = dirname(fileURLToPath(import.meta.url));

function json(data, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' },
  });
}

function problem(status, code, title, detail) {
  return new Response(JSON.stringify({
    type: `https://novnow.hub/problems/${code}`,
    title,
    status,
    detail,
    code,
    traceId: randomUUID(),
  }), {
    status,
    headers: { 'content-type': 'application/problem+json; charset=utf-8', 'cache-control': 'no-store' },
  });
}

function decodePathPart(value) {
  try {
    return decodeURIComponent(value);
  } catch {
    throw new ChatPortError(400, 'validation_failed', 'URL パラメーターが不正です。');
  }
}

async function readJson(request) {
  try {
    const value = await request.json();
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      throw new Error('Expected an object');
    }
    return value;
  } catch {
    throw new ChatPortError(400, 'validation_failed', 'JSON オブジェクトを指定してください。');
  }
}

/**
 * Create the HTTP surface for the ChatPort. `resolveActor` is optional on read
 * routes (which remain anonymously readable per OpenAPI), but required for writes.
 */
export function createChatApi({
  chatPort,
  resolveActor = () => undefined,
  isProjectOwner = () => false,
  prepareProject = () => {},
}) {
  if (!chatPort) throw new TypeError('chatPort is required');

  return async function handleChatRequest(request) {
    const url = new URL(request.url);
    const channelCollection = url.pathname.match(/^\/projects\/([^/]+)\/chat\/channels\/?$/);
    const messageCollection = url.pathname.match(/^\/projects\/([^/]+)\/chat\/channels\/([^/]+)\/messages\/?$/);
    const messageItem = url.pathname.match(/^\/projects\/([^/]+)\/chat\/channels\/([^/]+)\/messages\/([^/]+)\/?$/);

    try {
      const actorHandle = await resolveActor(request);
      if (channelCollection) {
        const projectId = decodePathPart(channelCollection[1]);
        if (request.method === 'GET') {
          prepareProject(projectId);
          return json(chatPort.listRooms(projectId, { actorHandle }));
        }
        if (request.method === 'POST') {
          if (!actorHandle) throw new ChatPortError(401, 'authentication_required', 'サインインが必要です。');
          const body = await readJson(request);
          const channel = chatPort.createRoom(projectId, {
            name: body.name,
            description: body.description,
          });
          return json(channel, 201);
        }
      }

      if (messageCollection) {
        const projectId = decodePathPart(messageCollection[1]);
        const channelId = decodePathPart(messageCollection[2]);
        if (request.method === 'GET') {
          const rawLimit = url.searchParams.get('limit');
          const limit = rawLimit === null ? 20 : Number(rawLimit);
          return json(chatPort.listMessages(projectId, channelId, {
            cursor: url.searchParams.get('cursor') ?? undefined,
            limit,
            actorHandle,
          }));
        }
        if (request.method === 'POST') {
          if (!actorHandle) throw new ChatPortError(401, 'authentication_required', 'サインインが必要です。');
          const body = await readJson(request);
          return json(chatPort.sendMessage(projectId, channelId, {
            body: body.body,
            authorHandle: actorHandle,
          }), 201);
        }
      }

      if (messageItem) {
        const projectId = decodePathPart(messageItem[1]);
        const channelId = decodePathPart(messageItem[2]);
        const messageId = decodePathPart(messageItem[3]);
        if (request.method === 'PATCH') {
          if (!actorHandle) throw new ChatPortError(401, 'authentication_required', 'サインインが必要です。');
          const body = await readJson(request);
          return json(chatPort.editMessage(projectId, channelId, messageId, {
            body: body.body,
            actorHandle,
          }));
        }
        if (request.method === 'DELETE') {
          if (!actorHandle) throw new ChatPortError(401, 'authentication_required', 'サインインが必要です。');
          const projectOwner = await isProjectOwner(request, projectId, actorHandle);
          chatPort.deleteMessage(projectId, channelId, messageId, { actorHandle, isProjectOwner: projectOwner });
          return new Response(null, { status: 204, headers: { 'cache-control': 'no-store' } });
        }
      }

      return problem(404, 'not_found', '見つかりません', '要求したチャット API が見つかりません。');
    } catch (error) {
      if (error instanceof ChatPortError) {
        const titles = {
          validation_failed: '入力内容に誤りがあります',
          authentication_required: 'サインインが必要です',
          permission_denied: '操作する権限がありません',
          not_found: '見つかりません',
        };
        return problem(error.status, error.code, titles[error.code] ?? '要求を処理できません', error.message);
      }
      return problem(500, 'internal_error', '内部エラーが発生しました', 'チャット API を処理できませんでした。');
    }
  };
}

async function readNodeRequestBody(request, maxBytes = 64 * 1024) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > maxBytes) throw new ChatPortError(413, 'payload_too_large', 'リクエストが大きすぎます。');
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString('utf8');
}

function sendNodeResponse(nodeResponse, response) {
  nodeResponse.statusCode = response.status;
  response.headers.forEach((value, name) => nodeResponse.setHeader(name, value));
  if (response.status === 204) {
    nodeResponse.end();
    return;
  }
  void response.arrayBuffer().then((body) => nodeResponse.end(Buffer.from(body)));
}

function resolveLocalDemoActor(request, enabled) {
  if (!enabled || process.env.NODE_ENV === 'production') return undefined;
  return request.headers.get('authorization') === 'Bearer mock-user-token' ? 'sora' : undefined;
}

function startServer() {
  const port = Number(process.env.HUB_CHAT_PORT ?? 4011);
  const host = process.env.HUB_CHAT_HOST ?? '0.0.0.0';
  const databasePath = process.env.HUB_CHAT_DB_PATH ?? resolve(here, '../.data/chat.sqlite');
  const seedDemo = process.argv.includes('--seed-demo');
  const demoAuth = process.argv.includes('--demo-auth');
  const chatPort = new SqliteChatPort({ databasePath });
  const handleChatRequest = createChatApi({
    chatPort,
    resolveActor: (request) => resolveLocalDemoActor(request, demoAuth),
    isProjectOwner: (_request, _projectId, actorHandle) =>
      demoAuth && process.env.NODE_ENV !== 'production' && actorHandle === 'sora',
    prepareProject: seedDemo ? (projectId) => chatPort.seedDemoProject(projectId) : undefined,
  });
  const server = createServer(async (request, response) => {
    try {
      const body = request.method === 'GET' || request.method === 'HEAD'
        ? undefined
        : await readNodeRequestBody(request);
      const webRequest = new Request(new URL(request.url ?? '/', `http://${request.headers.host ?? 'localhost'}`), {
        method: request.method,
        headers: request.headers,
        ...(body === undefined ? {} : { body }),
      });
      sendNodeResponse(response, await handleChatRequest(webRequest));
    } catch (error) {
      const failure = error instanceof ChatPortError
        ? problem(error.status, error.code, 'リクエストを処理できません', error.message)
        : problem(500, 'internal_error', '内部エラーが発生しました', 'チャット API を処理できませんでした。');
      sendNodeResponse(response, failure);
    }
  });
  server.listen(port, host, () => {
    process.stdout.write(`Chat API listening on http://${host}:${port} (SQLite: ${databasePath})\n`);
  });
  const shutdown = () => server.close(() => {
    chatPort.close();
    process.exit(0);
  });
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
  return server;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  startServer();
}
