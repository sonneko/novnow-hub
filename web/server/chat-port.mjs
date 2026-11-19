import { randomUUID } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

const deletedBody = '［削除されました］';

export class ChatPortError extends Error {
  constructor(status, code, message) {
    super(message);
    this.name = 'ChatPortError';
    this.status = status;
    this.code = code;
  }
}

function invalid(message) {
  throw new ChatPortError(400, 'validation_failed', message);
}

function validateProjectId(projectId) {
  if (typeof projectId !== 'string' || projectId.trim().length === 0 || [...projectId].length > 64) {
    invalid('プロジェクト ID が不正です。');
  }
  return projectId.trim();
}

function validateText(value, label, maxLength, { allowEmpty = false, trim = false } = {}) {
  if (typeof value !== 'string') invalid(`${label}は文字列で指定してください。`);
  const result = trim ? value.trim() : value;
  const length = [...result].length;
  if ((!allowEmpty && length === 0) || length > maxLength) {
    invalid(`${label}は1〜${maxLength}文字で指定してください。`);
  }
  return result;
}

function normalizeHandle(value) {
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new ChatPortError(401, 'authentication_required', 'サインインが必要です。');
  }
  const handle = value.trim();
  if ([...handle].length > 80) invalid('表示名が長すぎます。');
  return handle;
}

function extractMentions(body) {
  const handles = new Map();
  const expression = /(?<![\w.+-])@([\p{L}\p{N}_][\p{L}\p{N}_.-]{0,63})/gu;
  for (const match of body.matchAll(expression)) {
    const handle = match[1].replace(/[.-]+$/u, '');
    if (!handle) continue;
    const key = handle.toLocaleLowerCase('und');
    if (!handles.has(key)) handles.set(key, handle);
  }
  return [...handles.values()];
}

function encodeCursor(row) {
  return Buffer.from(JSON.stringify({ createdAt: row.created_at, id: row.id }), 'utf8').toString('base64url');
}

function decodeCursor(cursor) {
  if (typeof cursor !== 'string' || cursor.length > 512) invalid('カーソルが不正です。');
  try {
    const parsed = JSON.parse(Buffer.from(cursor, 'base64url').toString('utf8'));
    if (
      typeof parsed?.createdAt !== 'string' ||
      typeof parsed?.id !== 'string' ||
      Number.isNaN(Date.parse(parsed.createdAt))
    ) {
      invalid('カーソルが不正です。');
    }
    return { createdAt: parsed.createdAt, id: parsed.id };
  } catch (error) {
    if (error instanceof ChatPortError) throw error;
    invalid('カーソルが不正です。');
  }
}

function channelFromRow(row, unreadCount = 0) {
  return {
    id: row.id,
    name: row.name,
    description: row.description,
    unreadCount,
    createdAt: row.created_at,
  };
}

function messageFromRow(row) {
  return {
    id: row.id,
    channelId: row.channel_id,
    authorHandle: row.author_handle,
    body: row.body,
    mentions: JSON.parse(row.mentions_json),
    editedAt: row.edited_at,
    deleted: Boolean(row.deleted),
    createdAt: row.created_at,
  };
}

function comparePosition(left, right) {
  if (left.createdAt !== right.createdAt) return left.createdAt.localeCompare(right.createdAt);
  return left.id.localeCompare(right.id);
}

/** SQLite-backed hub store. It intentionally has no knowledge of external hosting providers. */
export class SqliteChatPort {
  constructor({ database, databasePath, clock = () => new Date().toISOString(), idFactory = randomUUID, eventBus = new EventEmitter() } = {}) {
    if (!database && !databasePath) {
      databasePath = resolve(process.cwd(), '.data', 'chat.sqlite');
    }
    if (!database && databasePath !== ':memory:') {
      mkdirSync(dirname(resolve(databasePath)), { recursive: true });
    }
    this.database = database ?? new DatabaseSync(databasePath);
    this.clock = clock;
    this.idFactory = idFactory;
    this.eventBus = eventBus;
    this.database.exec('PRAGMA foreign_keys = ON;');
    this.database.exec(`
      CREATE TABLE IF NOT EXISTS chat_channels (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        name TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL
      );
      CREATE INDEX IF NOT EXISTS chat_channels_by_project
        ON chat_channels(project_id, created_at, id);

      CREATE TABLE IF NOT EXISTS chat_messages (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        channel_id TEXT NOT NULL REFERENCES chat_channels(id) ON DELETE CASCADE,
        author_handle TEXT NOT NULL,
        body TEXT NOT NULL,
        mentions_json TEXT NOT NULL DEFAULT '[]',
        edited_at TEXT,
        deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
        created_at TEXT NOT NULL
      );
      CREATE INDEX IF NOT EXISTS chat_messages_by_channel
        ON chat_messages(project_id, channel_id, created_at, id);
      CREATE INDEX IF NOT EXISTS chat_messages_by_author
        ON chat_messages(project_id, channel_id, author_handle, created_at);

      CREATE TABLE IF NOT EXISTS chat_room_reads (
        project_id TEXT NOT NULL,
        channel_id TEXT NOT NULL REFERENCES chat_channels(id) ON DELETE CASCADE,
        reader_handle TEXT NOT NULL,
        last_read_at TEXT NOT NULL,
        last_read_message_id TEXT NOT NULL,
        PRIMARY KEY (project_id, channel_id, reader_handle)
      );

      CREATE TABLE IF NOT EXISTS chat_message_revisions (
        id TEXT PRIMARY KEY,
        message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
        editor_handle TEXT NOT NULL,
        body TEXT NOT NULL,
        edited_at TEXT NOT NULL
      );
      CREATE INDEX IF NOT EXISTS chat_message_revisions_by_message
        ON chat_message_revisions(message_id, edited_at, id);

      CREATE TABLE IF NOT EXISTS chat_notification_outbox (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        recipient_handle TEXT NOT NULL,
        project_id TEXT NOT NULL,
        channel_id TEXT NOT NULL,
        message_id TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        published_at TEXT
      );
      CREATE INDEX IF NOT EXISTS chat_notification_outbox_pending
        ON chat_notification_outbox(published_at, created_at);

      CREATE TABLE IF NOT EXISTS chat_demo_seeded_projects (
        project_id TEXT PRIMARY KEY,
        seeded_at TEXT NOT NULL
      );
    `);
  }

  #transaction(callback) {
    this.database.exec('BEGIN IMMEDIATE');
    try {
      const result = callback();
      this.database.exec('COMMIT');
      return result;
    } catch (error) {
      this.database.exec('ROLLBACK');
      throw error;
    }
  }

  #room(projectId, channelId) {
    const row = this.database
      .prepare('SELECT id, project_id, name, description, created_at FROM chat_channels WHERE project_id = ? AND id = ?')
      .get(projectId, channelId);
    if (!row) throw new ChatPortError(404, 'not_found', 'チャットルームが見つかりません。');
    return row;
  }

  #message(projectId, channelId, messageId) {
    const row = this.database
      .prepare(`SELECT id, project_id, channel_id, author_handle, body, mentions_json, edited_at, deleted, created_at
        FROM chat_messages WHERE project_id = ? AND channel_id = ? AND id = ?`)
      .get(projectId, channelId, messageId);
    if (!row) throw new ChatPortError(404, 'not_found', 'メッセージが見つかりません。');
    return row;
  }

  #recordMentionEvents({ projectId, room, message, recipients, createdAt }) {
    const events = [];
    for (const recipientHandle of recipients) {
      if (recipientHandle.toLocaleLowerCase('und') === message.authorHandle.toLocaleLowerCase('und')) continue;
      const event = {
        id: this.idFactory(),
        kind: 'mention',
        recipientHandle,
        projectId,
        channelId: room.id,
        messageId: message.id,
        title: `${message.authorHandle} さんがあなたにメンションしました`,
        body: `${room.name}: ${message.body}`.slice(0, 500),
        createdAt,
      };
      this.database.prepare(`INSERT INTO chat_notification_outbox
        (id, kind, recipient_handle, project_id, channel_id, message_id, payload_json, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)`)
        .run(event.id, event.kind, event.recipientHandle, event.projectId, event.channelId,
          event.messageId, JSON.stringify(event), createdAt);
      events.push(event);
    }
    return events;
  }

  #publish(events) {
    for (const event of events) {
      try {
        const delivered = this.eventBus.emit('notification', event);
        if (delivered) {
          this.database.prepare('UPDATE chat_notification_outbox SET published_at = ? WHERE id = ?')
            .run(this.clock(), event.id);
        }
      } catch {
        // Keep the outbox row pending if a downstream notification subscriber fails.
      }
    }
  }

  #markRead(projectId, channelId, readerHandle, position) {
    if (!position) return;
    const existing = this.database.prepare(`SELECT last_read_at, last_read_message_id
      FROM chat_room_reads WHERE project_id = ? AND channel_id = ? AND reader_handle = ?`)
      .get(projectId, channelId, readerHandle);
    const nextPosition = { createdAt: position.created_at, id: position.id };
    if (existing && comparePosition(nextPosition, {
      createdAt: existing.last_read_at,
      id: existing.last_read_message_id,
    }) <= 0) return;
    this.database.prepare(`INSERT INTO chat_room_reads
      (project_id, channel_id, reader_handle, last_read_at, last_read_message_id)
      VALUES (?, ?, ?, ?, ?)
      ON CONFLICT(project_id, channel_id, reader_handle) DO UPDATE SET
        last_read_at = excluded.last_read_at,
        last_read_message_id = excluded.last_read_message_id`)
      .run(projectId, channelId, readerHandle, nextPosition.createdAt, nextPosition.id);
  }

  createRoom(projectIdValue, { name, description = '' } = {}) {
    const projectId = validateProjectId(projectIdValue);
    const normalizedName = validateText(name, 'ルーム名', 80, { trim: true });
    const normalizedDescription = validateText(description, '説明', 200, { allowEmpty: true, trim: true });
    const row = {
      id: this.idFactory(),
      project_id: projectId,
      name: normalizedName,
      description: normalizedDescription,
      created_at: this.clock(),
    };
    this.database.prepare(`INSERT INTO chat_channels (id, project_id, name, description, created_at)
      VALUES (?, ?, ?, ?, ?)`)
      .run(row.id, row.project_id, row.name, row.description, row.created_at);
    return channelFromRow(row);
  }

  listRooms(projectIdValue, { actorHandle } = {}) {
    const projectId = validateProjectId(projectIdValue);
    const rows = this.database.prepare(`SELECT id, project_id, name, description, created_at
      FROM chat_channels WHERE project_id = ? ORDER BY created_at ASC, id ASC`)
      .all(projectId);
    const readerHandle = actorHandle ? normalizeHandle(actorHandle) : undefined;
    return rows.map((row) => {
      if (!readerHandle) return channelFromRow(row, 0);
      const readPosition = this.database.prepare(`SELECT last_read_at, last_read_message_id
        FROM chat_room_reads WHERE project_id = ? AND channel_id = ? AND reader_handle = ?`)
        .get(projectId, row.id, readerHandle);
      const unread = readPosition
        ? this.database.prepare(`SELECT COUNT(*) AS count FROM chat_messages
            WHERE project_id = ? AND channel_id = ? AND deleted = 0 AND author_handle <> ?
              AND (created_at > ? OR (created_at = ? AND id > ?))`)
          .get(projectId, row.id, readerHandle, readPosition.last_read_at,
            readPosition.last_read_at, readPosition.last_read_message_id).count
        : this.database.prepare(`SELECT COUNT(*) AS count FROM chat_messages
            WHERE project_id = ? AND channel_id = ? AND deleted = 0 AND author_handle <> ?`)
          .get(projectId, row.id, readerHandle).count;
      return channelFromRow(row, Number(unread));
    });
  }

  sendMessage(projectIdValue, channelId, { body, authorHandle } = {}) {
    const projectId = validateProjectId(projectIdValue);
    const room = this.#room(projectId, channelId);
    const normalizedBody = validateText(body, '本文', 4000);
    if (!normalizedBody.trim()) invalid('本文を入力してください。');
    const author = normalizeHandle(authorHandle);
    const createdAt = this.clock();
    const mentions = extractMentions(normalizedBody);
    const message = {
      id: this.idFactory(),
      channelId: room.id,
      authorHandle: author,
      body: normalizedBody,
      mentions,
      editedAt: null,
      deleted: false,
      createdAt,
    };
    const events = this.#transaction(() => {
      this.database.prepare(`INSERT INTO chat_messages
        (id, project_id, channel_id, author_handle, body, mentions_json, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)`)
        .run(message.id, projectId, room.id, author, normalizedBody, JSON.stringify(mentions), createdAt);
      const recorded = this.#recordMentionEvents({ projectId, room, message, recipients: mentions, createdAt });
      this.#markRead(projectId, room.id, author, { created_at: createdAt, id: message.id });
      return recorded;
    });
    this.#publish(events);
    return message;
  }

  listMessages(projectIdValue, channelId, { cursor, limit = 20, actorHandle } = {}) {
    const projectId = validateProjectId(projectIdValue);
    this.#room(projectId, channelId);
    if (!Number.isInteger(limit) || limit < 1 || limit > 100) invalid('limit は1〜100で指定してください。');
    const actor = actorHandle ? normalizeHandle(actorHandle) : undefined;
    const cursorPosition = cursor ? decodeCursor(cursor) : undefined;
    const whereCursor = cursorPosition
      ? ' AND (created_at < ? OR (created_at = ? AND id < ?))'
      : '';
    const values = cursorPosition
      ? [projectId, channelId, cursorPosition.createdAt, cursorPosition.createdAt, cursorPosition.id, limit + 1]
      : [projectId, channelId, limit + 1];
    const rows = this.database.prepare(`SELECT id, channel_id, author_handle, body, mentions_json, edited_at, deleted, created_at
      FROM chat_messages WHERE project_id = ? AND channel_id = ?${whereCursor}
      ORDER BY created_at DESC, id DESC LIMIT ?`)
      .all(...values);
    const hasMore = rows.length > limit;
    const pageRows = rows.slice(0, limit);
    const nextCursor = hasMore && pageRows.length > 0 ? encodeCursor(pageRows[pageRows.length - 1]) : null;
    const latest = this.database.prepare(`SELECT id, created_at FROM chat_messages
      WHERE project_id = ? AND channel_id = ? ORDER BY created_at DESC, id DESC LIMIT 1`)
      .get(projectId, channelId);
    if (actor && !cursorPosition) this.#markRead(projectId, channelId, actor, latest);
    return {
      items: pageRows.reverse().map(messageFromRow),
      nextCursor,
    };
  }

  editMessage(projectIdValue, channelId, messageId, { body, actorHandle } = {}) {
    const projectId = validateProjectId(projectIdValue);
    const actor = normalizeHandle(actorHandle);
    const previous = this.#message(projectId, channelId, messageId);
    if (previous.deleted) throw new ChatPortError(404, 'not_found', 'メッセージが見つかりません。');
    if (previous.author_handle.toLocaleLowerCase('und') !== actor.toLocaleLowerCase('und')) {
      throw new ChatPortError(403, 'permission_denied', 'このメッセージを編集する権限がありません。');
    }
    const normalizedBody = validateText(body, '本文', 4000);
    if (!normalizedBody.trim()) invalid('本文を入力してください。');
    const room = this.#room(projectId, channelId);
    const editedAt = this.clock();
    const oldMentions = JSON.parse(previous.mentions_json);
    const mentions = extractMentions(normalizedBody);
    const oldKeys = new Set(oldMentions.map((handle) => handle.toLocaleLowerCase('und')));
    const newMentions = mentions.filter((handle) => !oldKeys.has(handle.toLocaleLowerCase('und')));
    const updated = {
      id: previous.id,
      channelId: previous.channel_id,
      authorHandle: previous.author_handle,
      body: normalizedBody,
      mentions,
      editedAt,
      deleted: false,
      createdAt: previous.created_at,
    };
    const events = this.#transaction(() => {
      this.database.prepare(`INSERT INTO chat_message_revisions
        (id, message_id, editor_handle, body, edited_at) VALUES (?, ?, ?, ?, ?)`)
        .run(this.idFactory(), messageId, actor, previous.body, editedAt);
      this.database.prepare(`UPDATE chat_messages SET body = ?, mentions_json = ?, edited_at = ?
        WHERE project_id = ? AND channel_id = ? AND id = ?`)
        .run(normalizedBody, JSON.stringify(mentions), editedAt, projectId, channelId, messageId);
      return this.#recordMentionEvents({
        projectId,
        room,
        message: updated,
        recipients: newMentions,
        createdAt: editedAt,
      });
    });
    this.#publish(events);
    return updated;
  }

  deleteMessage(projectIdValue, channelId, messageId, { actorHandle, isProjectOwner = false } = {}) {
    const projectId = validateProjectId(projectIdValue);
    const actor = normalizeHandle(actorHandle);
    const previous = this.#message(projectId, channelId, messageId);
    if (!isProjectOwner && previous.author_handle.toLocaleLowerCase('und') !== actor.toLocaleLowerCase('und')) {
      throw new ChatPortError(403, 'permission_denied', 'このメッセージを削除する権限がありません。');
    }
    if (!previous.deleted) {
      this.#transaction(() => {
        this.database.prepare(`UPDATE chat_messages SET body = ?, mentions_json = '[]', deleted = 1
          WHERE project_id = ? AND channel_id = ? AND id = ?`)
          .run(deletedBody, projectId, channelId, messageId);
      });
    }
  }

  listPendingNotificationEvents() {
    return this.database.prepare(`SELECT payload_json FROM chat_notification_outbox
      WHERE published_at IS NULL ORDER BY created_at ASC, id ASC`)
      .all().map((row) => JSON.parse(row.payload_json));
  }

  seedDemoProject(projectIdValue) {
    const projectId = validateProjectId(projectIdValue);
    const alreadySeeded = this.database.prepare('SELECT project_id FROM chat_demo_seeded_projects WHERE project_id = ?')
      .get(projectId);
    if (alreadySeeded) return;
    const events = this.#transaction(() => {
      const seededAt = this.clock();
      const rooms = [
        { id: this.idFactory(), name: '全体', description: 'プロジェクト全体の連絡' },
        { id: this.idFactory(), name: '制作', description: '制作に関する相談' },
        { id: this.idFactory(), name: '雑談', description: '気軽な会話' },
      ].map((room, index) => ({
        ...room,
        createdAt: new Date(Date.parse(seededAt) + index).toISOString(),
      }));
      for (const room of rooms) {
        this.database.prepare(`INSERT INTO chat_channels (id, project_id, name, description, created_at)
          VALUES (?, ?, ?, ?, ?)`)
          .run(room.id, projectId, room.name, room.description, room.createdAt);
      }
      const createdAt = new Date(Date.parse(seededAt) + rooms.length).toISOString();
      const sampleMessage = {
        id: this.idFactory(),
        channelId: rooms[2].id,
        authorHandle: 'mizu',
        body: 'この作品の雰囲気が好きです。@sora さん、次の章も楽しみにしています！',
        mentions: ['sora'],
        editedAt: null,
        deleted: false,
        createdAt,
      };
      this.database.prepare(`INSERT INTO chat_messages
        (id, project_id, channel_id, author_handle, body, mentions_json, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)`)
        .run(sampleMessage.id, projectId, rooms[2].id, sampleMessage.authorHandle,
          sampleMessage.body, JSON.stringify(sampleMessage.mentions), createdAt);
      const event = {
        id: this.idFactory(),
        kind: 'mention',
        recipientHandle: 'sora',
        projectId,
        channelId: rooms[2].id,
        messageId: sampleMessage.id,
        title: 'mizu さんがあなたにメンションしました',
        body: `${rooms[2].name}: ${sampleMessage.body}`.slice(0, 500),
        createdAt,
      };
      this.database.prepare(`INSERT INTO chat_notification_outbox
        (id, kind, recipient_handle, project_id, channel_id, message_id, payload_json, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)`)
        .run(event.id, event.kind, event.recipientHandle, event.projectId, event.channelId,
          event.messageId, JSON.stringify(event), createdAt);
      this.database.prepare('INSERT INTO chat_demo_seeded_projects (project_id, seeded_at) VALUES (?, ?)')
        .run(projectId, createdAt);
      return [event];
    });
    this.#publish(events);
  }

  close() {
    this.database.close();
  }
}
