<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { ChatChannel, ChatMessage } from '../../lib/api/types';
  import { projectPageHref } from '../../lib/links';

  interface Props {
    projectId: string;
    locale: Locale;
    m: Messages;
  }

  const pollIntervalMs = 5_000;
  const pageSize = 100;
  const devHeaders = import.meta.env.DEV ? { Authorization: 'Bearer mock-user-token' } : undefined;
  const currentHandle = import.meta.env.DEV ? 'sora' : '';
  let { projectId, locale, m }: Props = $props();
  let rooms = $state<ChatChannel[]>([]);
  let selectedChannelId = $state('');
  let messages = $state<ChatMessage[]>([]);
  let pageState = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let messageFailure = $state<ApiFailure>();
  let actionFailure = $state<ApiFailure>();
  let draft = $state('');
  let roomName = $state('');
  let roomDescription = $state('');
  let showRoomForm = $state(false);
  let creatingRoom = $state(false);
  let sending = $state(false);
  let loadingOlder = $state(false);
  let nextCursor = $state<string | null>(null);
  let editingMessageId = $state('');
  let editDraft = $state('');
  let pollInFlight = false;

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale === 'ja' ? 'ja-JP' : 'en-US', {
      dateStyle: 'medium',
      timeStyle: 'short',
    }).format(date);
  }

  function sortMessages(items: ChatMessage[]): ChatMessage[] {
    return [...items].sort((left, right) =>
      left.createdAt.localeCompare(right.createdAt) || left.id.localeCompare(right.id));
  }

  function mergeMessages(existing: ChatMessage[], incoming: ChatMessage[]): ChatMessage[] {
    const byId = new Map(existing.map((message) => [message.id, message]));
    for (const message of incoming) byId.set(message.id, message);
    return sortMessages([...byId.values()]);
  }

  function selectedRoom(): ChatChannel | undefined {
    return rooms.find((room) => room.id === selectedChannelId);
  }

  async function refreshRooms(selectFirst = false): Promise<boolean> {
    if (!projectId) {
      failure = 'not-found';
      pageState = 'error';
      return false;
    }
    try {
      const result = await apiClient.GET('/projects/{projectId}/chat/channels', {
        params: { path: { projectId } },
        headers: devHeaders,
      });
      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        pageState = 'error';
        return false;
      }
      rooms = result.data;
      if (selectFirst || !rooms.some((room) => room.id === selectedChannelId)) {
        selectedChannelId = rooms[0]?.id ?? '';
        messages = [];
        nextCursor = null;
      }
      if (pageState === 'loading' || pageState === 'error') pageState = 'ready';
      return true;
    } catch {
      failure = 'upstream-unavailable';
      pageState = 'error';
      return false;
    }
  }

  async function loadMessages({
    channelId = selectedChannelId,
    cursor,
    replace = false,
  }: { channelId?: string; cursor?: string; replace?: boolean } = {}): Promise<boolean> {
    if (!projectId || !channelId) {
      messages = [];
      nextCursor = null;
      return true;
    }
    try {
      const result = await apiClient.GET('/projects/{projectId}/chat/channels/{channelId}/messages', {
        params: {
          path: { projectId, channelId },
          query: { limit: pageSize, cursor },
        },
        headers: devHeaders,
      });
      if (result.error) {
        if (channelId === selectedChannelId) {
          messageFailure = mapApiFailure(result.error, result.response.status);
        }
        return false;
      }
      if (channelId !== selectedChannelId) return false;
      const keepOlderCursor = !replace && !cursor && messages.length > result.data.items.length;
      messages = replace ? sortMessages(result.data.items) : mergeMessages(messages, result.data.items);
      if (!keepOlderCursor) nextCursor = result.data.nextCursor;
      messageFailure = undefined;
      return true;
    } catch {
      if (channelId === selectedChannelId) messageFailure = 'upstream-unavailable';
      return false;
    }
  }

  async function selectRoom(room: ChatChannel) {
    if (selectedChannelId === room.id) return;
    selectedChannelId = room.id;
    messages = [];
    nextCursor = null;
    messageFailure = undefined;
    actionFailure = undefined;
    editingMessageId = '';
    await loadMessages({ channelId: room.id, replace: true });
    await refreshRooms();
  }

  async function poll() {
    if (pollInFlight || pageState !== 'ready') return;
    pollInFlight = true;
    try {
      const activeChannelId = selectedChannelId;
      await loadMessages({ channelId: activeChannelId });
      await refreshRooms();
    } finally {
      pollInFlight = false;
    }
  }

  async function createRoom(event: SubmitEvent) {
    event.preventDefault();
    if (!projectId || creatingRoom || !roomName.trim()) return;
    creatingRoom = true;
    actionFailure = undefined;
    try {
      const result = await apiClient.POST('/projects/{projectId}/chat/channels', {
        params: { path: { projectId } },
        headers: devHeaders,
        body: { name: roomName.trim(), description: roomDescription.trim() || undefined },
      });
      if (result.error) {
        actionFailure = mapApiFailure(result.error, result.response.status);
        return;
      }
      roomName = '';
      roomDescription = '';
      showRoomForm = false;
      rooms = [...rooms, result.data];
      selectedChannelId = result.data.id;
      messages = [];
      nextCursor = null;
      await loadMessages({ channelId: result.data.id, replace: true });
      await refreshRooms();
    } catch {
      actionFailure = 'upstream-unavailable';
    } finally {
      creatingRoom = false;
    }
  }

  async function sendMessage(event: SubmitEvent) {
    event.preventDefault();
    const body = draft.trim();
    if (!projectId || !selectedChannelId || !body || sending) return;
    sending = true;
    actionFailure = undefined;
    try {
      const result = await apiClient.POST('/projects/{projectId}/chat/channels/{channelId}/messages', {
        params: { path: { projectId, channelId: selectedChannelId } },
        headers: devHeaders,
        body: { body },
      });
      if (result.error) {
        actionFailure = mapApiFailure(result.error, result.response.status);
        return;
      }
      messages = mergeMessages(messages, [result.data]);
      draft = '';
      await refreshRooms();
    } catch {
      actionFailure = 'upstream-unavailable';
    } finally {
      sending = false;
    }
  }

  async function loadOlderMessages() {
    if (!nextCursor || loadingOlder) return;
    loadingOlder = true;
    const cursor = nextCursor;
    try {
      await loadMessages({ cursor });
    } finally {
      loadingOlder = false;
    }
  }

  function startEdit(message: ChatMessage) {
    editingMessageId = message.id;
    editDraft = message.body;
    actionFailure = undefined;
  }

  async function saveEdit(event: SubmitEvent) {
    event.preventDefault();
    const body = editDraft.trim();
    if (!projectId || !selectedChannelId || !editingMessageId || !body) return;
    actionFailure = undefined;
    try {
      const result = await apiClient.PATCH('/projects/{projectId}/chat/channels/{channelId}/messages/{messageId}', {
        params: { path: { projectId, channelId: selectedChannelId, messageId: editingMessageId } },
        headers: devHeaders,
        body: { body },
      });
      if (result.error) {
        actionFailure = mapApiFailure(result.error, result.response.status);
        return;
      }
      messages = mergeMessages(messages, [result.data]);
      editingMessageId = '';
      editDraft = '';
      await refreshRooms();
    } catch {
      actionFailure = 'upstream-unavailable';
    }
  }

  async function deleteMessage(message: ChatMessage) {
    if (!projectId || !selectedChannelId || message.deleted) return;
    actionFailure = undefined;
    try {
      const result = await apiClient.DELETE('/projects/{projectId}/chat/channels/{channelId}/messages/{messageId}', {
        params: { path: { projectId, channelId: selectedChannelId, messageId: message.id } },
        headers: devHeaders,
      });
      if (result.error) {
        actionFailure = mapApiFailure(result.error, result.response.status);
        return;
      }
      await loadMessages({ replace: true });
    } catch {
      actionFailure = 'upstream-unavailable';
    }
  }

  onMount(() => {
    let timer: ReturnType<typeof setInterval> | undefined;
    let active = true;
    void (async () => {
      const loaded = await refreshRooms(true);
      if (loaded && selectedChannelId) {
        await loadMessages({ replace: true });
        await refreshRooms();
      }
      if (active) timer = setInterval(() => void poll(), pollIntervalMs);
    })();
    return () => {
      active = false;
      if (timer) clearInterval(timer);
    };
  });
</script>

<section class="collaboration-page chat-page" aria-labelledby="chat-heading">
  <p class="breadcrumbs"><a href={projectPageHref(projectId)}>{m.collaboration.backToProject}</a></p>
  <header class="page-heading">
    <h1 id="chat-heading">{m.collaboration.chat.title}</h1>
    <p>{m.collaboration.chat.lead}</p>
  </header>

  {#if pageState === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if pageState === 'error'}
    <p role="alert">{m.api.errors[failure]}</p>
  {:else}
    <div class="chat-layout">
      <aside class="chat-sidebar panel" aria-label={m.collaboration.chat.roomsLabel}>
        <div class="chat-sidebar-heading">
          <h2>{m.collaboration.chat.rooms}</h2>
          <button class="button" type="button" onclick={() => (showRoomForm = !showRoomForm)}>
            {showRoomForm ? m.collaboration.chat.cancel : m.collaboration.chat.createRoom}
          </button>
        </div>
        {#if showRoomForm}
          <form class="chat-create-room" onsubmit={createRoom}>
            <label for="chat-room-name">{m.collaboration.chat.roomName}</label>
            <input id="chat-room-name" bind:value={roomName} maxlength="80" required />
            <label for="chat-room-description">{m.collaboration.chat.roomDescription}</label>
            <input id="chat-room-description" bind:value={roomDescription} maxlength="200" />
            <button class="button button-primary" type="submit" disabled={creatingRoom || !roomName.trim()}>
              {creatingRoom ? m.collaboration.chat.creatingRoom : m.collaboration.chat.createRoom}
            </button>
          </form>
        {/if}
        {#if rooms.length === 0}
          <p class="meta">{m.collaboration.chat.noRooms}</p>
        {:else}
          <ul class="chat-room-list">
            {#each rooms as room (room.id)}
              <li>
                <button
                  class="chat-room-button"
                  class:active={selectedChannelId === room.id}
                  type="button"
                  aria-current={selectedChannelId === room.id ? 'page' : undefined}
                  onclick={() => void selectRoom(room)}
                >
                  <span>{room.name}</span>
                  {#if room.unreadCount > 0}
                    <span class="badge chat-unread-badge" aria-label={`${room.unreadCount} ${m.collaboration.chat.unread}`}>
                      {room.unreadCount}
                    </span>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </aside>

      <section class="chat-conversation panel" aria-label={m.collaboration.chat.messagesLabel}>
        <header class="chat-conversation-heading">
          <h2>{selectedRoom()?.name ?? m.collaboration.chat.chooseRoom}</h2>
          <p class="meta">{m.collaboration.chat.pollingNotice}</p>
        </header>

        {#if !selectedChannelId}
          <p>{m.collaboration.chat.chooseRoom}</p>
        {:else if messageFailure}
          <p role="alert">{m.api.errors[messageFailure]}</p>
        {:else}
          {#if nextCursor}
            <div class="chat-load-older">
              <button class="button" type="button" disabled={loadingOlder} onclick={() => void loadOlderMessages()}>
                {loadingOlder ? m.collaboration.chat.loadingOlder : m.collaboration.chat.loadOlder}
              </button>
            </div>
          {/if}
          {#if messages.length === 0}
            <p class="meta">{m.collaboration.chat.noMessages}</p>
          {:else}
            <ol class="chat-message-list" aria-live="polite">
              {#each messages as message (message.id)}
                <li class="chat-message" class:chat-message-deleted={message.deleted} data-message-id={message.id}>
                  <div class="chat-message-heading">
                    <strong>{message.authorHandle}</strong>
                    <time datetime={message.createdAt}>{formatDate(message.createdAt)}</time>
                    {#if message.editedAt && !message.deleted}<span class="meta">{m.collaboration.chat.edited}</span>{/if}
                  </div>
                  {#if editingMessageId === message.id}
                    <form class="chat-edit-form" onsubmit={saveEdit}>
                      <label class="visually-hidden" for={`edit-message-${message.id}`}>{m.collaboration.chat.editMessage}</label>
                      <textarea id={`edit-message-${message.id}`} bind:value={editDraft} maxlength="4000" required></textarea>
                      <button class="button button-primary" type="submit" disabled={!editDraft.trim()}>{m.collaboration.chat.save}</button>
                      <button class="button" type="button" onclick={() => (editingMessageId = '')}>{m.collaboration.chat.cancel}</button>
                    </form>
                  {:else}
                    <p class="chat-message-body">{message.body}</p>
                    {#if message.mentions?.length && !message.deleted}
                      <p class="chat-mentions">{m.collaboration.chat.mentions}: {message.mentions.map((handle) => `@${handle}`).join(', ')}</p>
                    {/if}
                  {/if}
                  {#if !message.deleted && currentHandle && message.authorHandle.toLocaleLowerCase() === currentHandle}
                    <div class="chat-message-actions">
                      {#if editingMessageId !== message.id}
                        <button class="button" type="button" onclick={() => startEdit(message)}>{m.collaboration.chat.editMessage}</button>
                      {/if}
                      <button class="button button-danger" type="button" onclick={() => void deleteMessage(message)}>{m.collaboration.chat.deleteMessage}</button>
                    </div>
                  {/if}
                </li>
              {/each}
            </ol>
          {/if}
        {/if}

        {#if actionFailure}
          <p class="chat-action-error" role="alert">{m.api.errors[actionFailure]}</p>
        {/if}
        {#if selectedChannelId}
          <form class="chat-compose" onsubmit={sendMessage}>
            <label for="chat-message-body">{m.collaboration.chat.messageLabel}</label>
            <p id="chat-mention-help" class="meta">{m.collaboration.chat.mentionHelp}</p>
            <textarea
              id="chat-message-body"
              bind:value={draft}
              maxlength="4000"
              aria-describedby="chat-mention-help"
              required
            ></textarea>
            <div class="chat-compose-footer">
              <span class="meta">{draft.length}/4000</span>
              <button class="button button-primary" type="submit" disabled={sending || !draft.trim()}>
                {sending ? m.collaboration.chat.sending : m.collaboration.chat.send}
              </button>
            </div>
          </form>
        {/if}
      </section>
    </div>
  {/if}
</section>
