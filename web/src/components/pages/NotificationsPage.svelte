<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Notification, NotificationPageData } from '../../lib/api/types';

  interface Props {
    locale: Locale;
    m: Messages;
  }

  let { locale, m }: Props = $props();
  const devHeaders = $derived(
    import.meta.env.DEV ? { Authorization: 'Bearer mock-user-token' } : undefined,
  );

  let notifications = $state<Notification[]>([]);
  let pageState = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let actionFailure = $state<ApiFailure>();
  let notice = $state<'marked' | 'allRead'>();
  let kind = $state('');
  let unreadOnly = $state(false);
  let nextCursor = $state<string | null>(null);
  let hasMore = $state(false);
  let loadingMore = $state(false);
  let markingAll = $state(false);
  let markingId = $state('');

  const kindOptions = $derived(Object.keys(m.collaboration.notifications.kinds));

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale === 'ja' ? 'ja-JP' : 'en-US', {
      dateStyle: 'medium',
      timeStyle: 'short',
    }).format(date);
  }

  function kindLabel(value: string): string {
    return m.collaboration.notifications.kinds[value as keyof typeof m.collaboration.notifications.kinds] ?? value;
  }

  function unreadCount(items: Notification[]): number {
    return items.filter((item) => !item.read).length;
  }

  async function load(cursor?: string): Promise<boolean> {
    try {
      const result = await apiClient.GET('/notifications', {
        params: {
          query: {
            kind: kind || undefined,
            unreadOnly: unreadOnly || undefined,
            limit: 20,
            cursor,
          },
        },
        headers: devHeaders,
      });
      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        pageState = 'error';
        return false;
      }
      const page: NotificationPageData = result.data;
      notifications = cursor ? [...notifications, ...page.items] : page.items;
      nextCursor = page.nextCursor;
      hasMore = page.nextCursor !== null && page.nextCursor !== undefined;
      pageState = 'ready';
      return true;
    } catch {
      failure = 'upstream-unavailable';
      pageState = 'error';
      return false;
    }
  }

  async function reload() {
    notifications = [];
    nextCursor = null;
    hasMore = false;
    notice = undefined;
    actionFailure = undefined;
    pageState = 'loading';
    await load();
  }

  async function applyFilters(event: SubmitEvent) {
    event.preventDefault();
    await reload();
  }

  async function markRead(item: Notification) {
    if (item.read || markingId) return;
    markingId = item.id;
    actionFailure = undefined;
    notice = undefined;
    try {
      const result = await apiClient.POST('/notifications/read', {
        headers: devHeaders,
        body: { ids: [item.id] },
      });
      if (result.error) {
        actionFailure = mapApiFailure(result.error, result.response.status);
        return;
      }
      item.read = true;
      notice = 'marked';
    } catch {
      actionFailure = 'upstream-unavailable';
    } finally {
      markingId = '';
    }
  }

  async function markAllRead() {
    if (markingAll) return;
    markingAll = true;
    actionFailure = undefined;
    notice = undefined;
    try {
      const result = await apiClient.POST('/notifications/read', {
        headers: devHeaders,
        body: { all: true },
      });
      if (result.error) {
        actionFailure = mapApiFailure(result.error, result.response.status);
        return;
      }
      for (const item of notifications) item.read = true;
      notice = 'allRead';
      await load();
    } catch {
      actionFailure = 'upstream-unavailable';
    } finally {
      markingAll = false;
    }
  }

  async function loadMore() {
    if (!hasMore || loadingMore || !nextCursor) return;
    loadingMore = true;
    try {
      await load(nextCursor);
    } finally {
      loadingMore = false;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<section class="collaboration-page notifications-page" aria-labelledby="notifications-heading">
  <header class="page-heading">
    <h1 id="notifications-heading">{m.collaboration.notifications.title}</h1>
    <p>{m.collaboration.notifications.lead}</p>
  </header>

  {#if unreadCount(notifications) > 0}
    <p class="notifications-unread-summary" role="status">
      {m.collaboration.notifications.unreadCount(unreadCount(notifications))}
    </p>
  {/if}

  <form class="panel notifications-filter" onsubmit={applyFilters}>
    <label for="notification-kind">{m.collaboration.notifications.filter}</label>
    <select id="notification-kind" bind:value={kind}>
      <option value="">{m.collaboration.notifications.allKinds}</option>
      {#each kindOptions as option (option)}
        <option value={option}>{kindLabel(option)}</option>
      {/each}
    </select>
    <label class="notifications-unread-toggle">
      <input type="checkbox" bind:checked={unreadOnly} />
      {m.collaboration.notifications.unreadOnly}
    </label>
    <button class="button button-primary" type="submit">{m.collaboration.notifications.filter}</button>
  </form>

  {#if pageState === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if pageState === 'error'}
    <p role="alert">{m.api.errors[failure]}</p>
  {:else}
    {#if notifications.length === 0}
      <p class="meta">{m.collaboration.notifications.empty}</p>
    {:else}
      <ul class="notification-list">
        {#each notifications as item (item.id)}
          <li class="notification-item panel" class:notification-unread={!item.read}>
            <div class="notification-heading">
              <span class="badge">{kindLabel(item.kind)}</span>
              <span class="notification-read-state">
                {item.read ? m.collaboration.notifications.read : m.collaboration.notifications.unread}
              </span>
            </div>
            <h2>{item.title}</h2>
            <p class="notification-body">{item.body}</p>
            <div class="notification-meta">
              <time datetime={item.createdAt}>{formatDate(item.createdAt)}</time>
              {#if item.projectSlug}
                <span>{item.projectSlug}</span>
              {/if}
            </div>
            {#if !item.read}
              <button
                class="button"
                type="button"
                disabled={markingId === item.id}
                onclick={() => void markRead(item)}
              >
                {markingId === item.id ? m.collaboration.notifications.marking : m.collaboration.notifications.markRead}
              </button>
            {/if}
          </li>
        {/each}
      </ul>

      {#if hasMore}
        <div class="notifications-load-more">
          <button class="button" type="button" disabled={loadingMore} onclick={() => void loadMore()}>
            {loadingMore ? m.collaboration.notifications.loadingMore : m.collaboration.notifications.loadMore}
          </button>
        </div>
      {/if}
    {/if}
  {/if}

  {#if notice === 'marked'}
    <p class="review-success" role="status">{m.collaboration.notifications.markedRead}</p>
  {/if}
  {#if notice === 'allRead'}
    <p class="review-success" role="status">{m.collaboration.notifications.allRead}</p>
  {/if}
  {#if actionFailure}
    <p role="alert">{m.api.errors[actionFailure]}</p>
  {/if}

  {#if notifications.some((item) => !item.read)}
    <div class="panel notifications-mark-all">
      <button class="button button-primary" type="button" disabled={markingAll} onclick={() => void markAllRead()}>
        {markingAll ? m.collaboration.notifications.marking : m.collaboration.notifications.markAllRead}
      </button>
    </div>
  {/if}
</section>
