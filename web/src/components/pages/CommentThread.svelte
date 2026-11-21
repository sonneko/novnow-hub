<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { ApiComment } from '../../lib/api/types';

  interface Props {
    projectId: string;
    number: number;
    kind: 'proposal' | 'issue';
    locale: Locale;
    m: Messages;
  }

  let { projectId, number, kind, locale, m }: Props = $props();
  let comments = $state<ApiComment[]>([]);
  let nextCursor = $state<string | null>(null);
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let loadingMore = $state(false);
  let moreFailure = $state<ApiFailure>();

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale === 'ja' ? 'ja-JP' : 'en-US', {
      dateStyle: 'medium',
      timeZone: 'UTC',
    }).format(date);
  }

  /** Fetch one page of the thread. The contract has separate GET operations per resource. */
  async function fetchPage(cursor?: string) {
    const query = { cursor, limit: 20 };
    return kind === 'proposal'
      ? apiClient.GET('/projects/{projectId}/proposals/{number}/comments', {
          params: { path: { projectId, number }, query },
        })
      : apiClient.GET('/projects/{projectId}/issues/{number}/comments', {
          params: { path: { projectId, number }, query },
        });
  }

  async function load() {
    try {
      const result = await fetchPage();
      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        state = 'error';
        return;
      }
      comments = result.data.items;
      nextCursor = result.data.nextCursor;
      state = 'ready';
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    }
  }

  async function loadMore() {
    if (!nextCursor || loadingMore) return;
    loadingMore = true;
    moreFailure = undefined;
    try {
      const result = await fetchPage(nextCursor);
      if (result.error) {
        moreFailure = mapApiFailure(result.error, result.response.status);
      } else {
        comments = [...comments, ...result.data.items];
        nextCursor = result.data.nextCursor;
      }
    } catch {
      moreFailure = 'upstream-unavailable';
    } finally {
      loadingMore = false;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<section class="panel comment-thread" aria-labelledby={`${kind}-comments-heading`}>
  <h2 id={`${kind}-comments-heading`}>{m.collaboration.comments.title}</h2>
  {#if state === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <p role="alert">{m.collaboration.comments.unavailable}</p>
    <p class="meta">{m.api.errors[failure]}</p>
  {:else if comments.length === 0}
    <p>{m.collaboration.comments.empty}</p>
  {:else}
    <ol class="comment-list">
      {#each comments as comment (comment.id)}
        <li class="comment-item">
          <div class="comment-meta">
            <strong>{comment.authorHandle}</strong>
            <time datetime={comment.createdAt}>{formatDate(comment.createdAt)}</time>
            {#if comment.path}
              <span class="comment-location">
                {comment.path}{#if comment.line}（{m.collaboration.comments.lineRef(comment.line)}）{/if}
              </span>
            {/if}
          </div>
          <p class="comment-body">{comment.body}</p>
        </li>
      {/each}
    </ol>
    {#if moreFailure}
      <p role="alert">{m.api.errors[moreFailure]}</p>
    {/if}
    {#if nextCursor}
      <button class="button" type="button" onclick={() => void loadMore()} disabled={loadingMore}>
        {loadingMore ? m.collaboration.loadingMore : m.collaboration.loadMore}
      </button>
    {/if}
  {/if}
</section>
