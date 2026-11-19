import type { ChatChannel, ChatMessage, ChatMessagePage } from '../src/lib/api/types.js';
import type { DatabaseSync } from 'node:sqlite';
import type { EventEmitter } from 'node:events';

export interface ChatPort {
  createRoom(projectId: string, input: { name: string; description?: string }): ChatChannel;
  listRooms(projectId: string, options?: { actorHandle?: string }): ChatChannel[];
  sendMessage(projectId: string, channelId: string, input: { body: string; authorHandle: string }): ChatMessage;
  listMessages(projectId: string, channelId: string, options?: {
    cursor?: string;
    limit?: number;
    actorHandle?: string;
  }): ChatMessagePage;
  editMessage(projectId: string, channelId: string, messageId: string, input: {
    body: string;
    actorHandle: string;
  }): ChatMessage;
  deleteMessage(projectId: string, channelId: string, messageId: string, input: {
    actorHandle: string;
    isProjectOwner?: boolean;
  }): void;
}

export interface ChatNotificationEvent {
  id: string;
  kind: 'mention';
  recipientHandle: string;
  projectId: string;
  channelId: string;
  messageId: string;
  title: string;
  body: string;
  createdAt: string;
}

export interface SqliteChatPortOptions {
  database?: DatabaseSync;
  databasePath?: string;
  clock?: () => string;
  idFactory?: () => string;
  eventBus?: EventEmitter;
}

export class ChatPortError extends Error {
  status: number;
  code: string;
  constructor(status: number, code: string, message: string);
}

export class SqliteChatPort implements ChatPort {
  database: DatabaseSync;
  eventBus: EventEmitter;
  constructor(options?: SqliteChatPortOptions);
  createRoom(projectId: string, input: { name: string; description?: string }): ChatChannel;
  listRooms(projectId: string, options?: { actorHandle?: string }): ChatChannel[];
  sendMessage(projectId: string, channelId: string, input: { body: string; authorHandle: string }): ChatMessage;
  listMessages(projectId: string, channelId: string, options?: {
    cursor?: string;
    limit?: number;
    actorHandle?: string;
  }): ChatMessagePage;
  editMessage(projectId: string, channelId: string, messageId: string, input: {
    body: string;
    actorHandle: string;
  }): ChatMessage;
  deleteMessage(projectId: string, channelId: string, messageId: string, input: {
    actorHandle: string;
    isProjectOwner?: boolean;
  }): void;
  listPendingNotificationEvents(): ChatNotificationEvent[];
  seedDemoProject(projectId: string): void;
  close(): void;
}
