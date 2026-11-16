<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Issue } from '../../lib/api/types';
  import { issueDetailHref, projectPageHref } from '../../lib/links';

  interface Props {
    projectId: string;
    locale: Locale;
    m: Messages;
  }

  const pageSize = 20;
  let { projectId, locale, m }: Props = $props();
  let issues = $state<Issue[]>([]);
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let filterInput = $state('');
  let activeState = $state<Issue['state']>();
  let labelInput = $state('');
  let activeLabel = $state('');
  let nextCursor = $state<string | null>(null);
  let loadingMore = $state(false);

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale === 'ja' ? 'ja-JP' : 'en-US', {
      dateStyle: 'medium',
      timeZone: 'UTC',
    }).format(date);
  }

  async function load(cursor?: string) {
    if (!projectId) {
      failure = 'not-found';
      state = 'error';
      return;
    }
    if (cursor) loadingMore = true;
    else state = 'loading';

    try {
      const result = await apiClient.GET('/projects/{projectId}/issues', {
        params: {
          path: { projectId },
          query: { state: activeState, label: activeLabel || undefined, cursor, limit: pageSize },
        },
      });
      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        state = 'error';
        return;
      }
      const byNumber = new Map(issues.map((issue) => [issue.number, issue]));
      for (const issue of result.data.items) byNumber.set(issue.number, issue);
      issues = [...byNumber.values()];
      nextCursor = result.data.nextCursor;
      state = 'ready';
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    } finally {
      loadingMore = false;
    }
  }

  function applyFilter(event: SubmitEvent) {
    event.preventDefault();
    activeState = filterInput ? (filterInput as Issue['state']) : undefined;
    activeLabel = labelInput.trim();
    issues = [];
    nextCursor = null;
    void load();
  }

  function loadMore() {
    if (nextCursor && !loadingMore) void load(nextCursor);
  }

  onMount(() => {
    void load();
  });
</script>

<section class="collaboration-page" aria-labelledby="issues-heading">
  <p class="breadcrumbs"><a href={projectPageHref(projectId)}>{m.collaboration.backToProject}</a></p>
  <header class="page-heading">
    <h1 id="issues-heading">{m.collaboration.issues.title}</h1>
    <p>{m.collaboration.issues.lead}</p>
  </header>

  <form class="collaboration-filter panel" onsubmit={applyFilter}>
    <div>
      <label for="issue-state">{m.collaboration.issues.filter}</label>
      <select id="issue-state" bind:value={filterInput}>
        <option value="">{m.collaboration.issues.allStates}</option>
        <option value="open">{m.collaboration.issues.open}</option>
        <option value="closed">{m.collaboration.issues.closed}</option>
      </select>
    </div>
    <div>
      <label for="issue-label">{m.collaboration.issues.labels}</label>
      <input id="issue-label" bind:value={labelInput} maxlength="50" />
    </div>
    <button class="button button-primary" type="submit">{m.projects.search.submit}</button>
  </form>

  {#if state === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <p role="alert">{m.api.errors[failure]}</p>
  {:else if issues.length === 0}
    <p>{m.collaboration.issues.empty}</p>
  {:else}
    <ul class="collaboration-list">
      {#each issues as issue (issue.number)}
        <li class="panel collaboration-card">
          <div class="collaboration-card-top">
            <span class="collaboration-number">#{issue.number}</span>
            <span class="badge">{m.collaboration.issues[issue.state]}</span>
          </div>
          <h2><a href={issueDetailHref(projectId, issue.number)}>{issue.title}</a></h2>
          <p class="collaboration-summary">{issue.body || m.collaboration.emptyDescription}</p>
          {#if issue.labels?.length}
            <ul class="collaboration-tags" aria-label={m.collaboration.issues.labels}>
              {#each issue.labels as label (label)}<li>{label}</li>{/each}
            </ul>
          {/if}
          <div class="collaboration-meta">
            <span>{m.collaboration.issues.author}: {issue.authorHandle}</span>
            <time datetime={issue.updatedAt}>{m.collaboration.updated}: {formatDate(issue.updatedAt)}</time>
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  {#if state === 'ready' && nextCursor}
    <div class="collaboration-pagination">
      <button class="button" type="button" onclick={loadMore} disabled={loadingMore}>
        {loadingMore ? m.collaboration.loadingMore : m.collaboration.loadMore}
      </button>
    </div>
  {/if}
</section>
