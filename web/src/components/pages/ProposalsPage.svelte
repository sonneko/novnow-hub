<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Proposal } from '../../lib/api/types';
  import { proposalDetailHref, projectPageHref } from '../../lib/links';

  interface Props {
    projectId: string;
    locale: Locale;
    m: Messages;
  }

  const pageSize = 20;
  let { projectId, locale, m }: Props = $props();
  let proposals = $state<Proposal[]>([]);
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let filterInput = $state('');
  let activeState = $state<Proposal['state']>();
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
      const result = await apiClient.GET('/projects/{projectId}/proposals', {
        params: {
          path: { projectId },
          query: { state: activeState, cursor, limit: pageSize },
        },
      });
      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        state = 'error';
        return;
      }
      const byNumber = new Map(proposals.map((proposal) => [proposal.number, proposal]));
      for (const proposal of result.data.items) byNumber.set(proposal.number, proposal);
      proposals = [...byNumber.values()];
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
    activeState = filterInput ? (filterInput as Proposal['state']) : undefined;
    proposals = [];
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

<section class="collaboration-page" aria-labelledby="proposals-heading">
  <p class="breadcrumbs"><a href={projectPageHref(projectId)}>{m.collaboration.backToProject}</a></p>
  <header class="page-heading">
    <h1 id="proposals-heading">{m.collaboration.proposals.title}</h1>
    <p>{m.collaboration.proposals.lead}</p>
  </header>

  <form class="collaboration-filter panel" onsubmit={applyFilter}>
    <label for="proposal-state">{m.collaboration.proposals.filter}</label>
    <select id="proposal-state" bind:value={filterInput}>
      <option value="">{m.collaboration.proposals.allStates}</option>
      <option value="open">{m.collaboration.proposals.open}</option>
      <option value="approved">{m.collaboration.proposals.approved}</option>
      <option value="merged">{m.collaboration.proposals.merged}</option>
      <option value="closed">{m.collaboration.proposals.closed}</option>
    </select>
    <button class="button button-primary" type="submit">{m.projects.search.submit}</button>
  </form>

  {#if state === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <p role="alert">{m.api.errors[failure]}</p>
  {:else if proposals.length === 0}
    <p>{m.collaboration.proposals.empty}</p>
  {:else}
    <ul class="collaboration-list">
      {#each proposals as proposal (proposal.number)}
        <li class="panel collaboration-card">
          <div class="collaboration-card-top">
            <span class="collaboration-number">#{proposal.number}</span>
            <span class="badge">{m.collaboration.proposals[proposal.state]}</span>
          </div>
          <h2><a href={proposalDetailHref(projectId, proposal.number)}>{proposal.title}</a></h2>
          <p class="collaboration-summary">{proposal.body || m.collaboration.emptyDescription}</p>
          <div class="collaboration-meta">
            <span>{m.collaboration.author}: {proposal.authorHandle}</span>
            <span>{m.collaboration.proposals.approvalProgress}: {proposal.approvalCount}/{proposal.requiredApprovals}</span>
            <time datetime={proposal.updatedAt}>{m.collaboration.updated}: {formatDate(proposal.updatedAt)}</time>
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
