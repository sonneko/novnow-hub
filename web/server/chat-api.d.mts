import type { ChatPort } from './chat-port.mjs';

export interface ChatApiOptions {
  chatPort: ChatPort;
  resolveActor?: (request: Request) => string | undefined | Promise<string | undefined>;
  isProjectOwner?: (request: Request, projectId: string, actorHandle: string) => boolean | Promise<boolean>;
  prepareProject?: (projectId: string) => void;
}

export function createChatApi(options: ChatApiOptions): (request: Request) => Promise<Response>;
