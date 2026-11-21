<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Issue } from '../../lib/api/types';
  import { issueListHref, proposalDetailHref, projectPageHref } from '../../lib/links';
  import CommentThread from './CommentThread.svelte';

  interface Props {
    projectId: string;
    number: number;
    locale: Locale;
    m: Messages;
  }

  let { projectId, number, locale, m }: Props = $props();
  let issue = $state<Issue>();
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale === 'ja' ? 'ja-JP' : 'en-US', {
      dateStyle: 'medium',
      timeZone: 'UTC',
    }).format(date);
  }

  async function load() {
    if (!projectId || number < 1) {
      failure = 'not-found';
      state = 'error';
      return;
    }
    try {
      const result = await apiClient.GET('/projects/{projectId}/issues/{number}', {
        params: { path: { projectId, number } },
      });
      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        state = 'error';
      } else {
        issue = result.data;
        state = 'ready';
      }
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    }
  }

  onMount(() => {
    void load();
  });
</script>

<section class="collaboration-page" aria-labelledby="issue-heading">
  <p class="breadcrumbs">
    <a href={projectPageHref(projectId)}>{m.collaboration.backToProject}</a>
    <span aria-hidden="true"> / </span>
    <a href={issueListHref(projectId)}>{m.collaboration.issues.title}</a>
  </p>

  {#if state === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <article class="panel">
      <h1 id="issue-heading">{m.collaboration.issues.title}</h1>
      <p role="alert">{m.api.errors[failure]}</p>
    </article>
  {:else if issue}
    <article class="panel issue-detail">
      <div class="collaboration-card-top">
        <span class="collaboration-number">#{issue.number}</span>
        <span class="badge">{m.collaboration.issues[issue.state]}</span>
      </div>
      <h1 id="issue-heading">{issue.title}</h1>
      <div class="collaboration-meta">
        <span>{m.collaboration.issues.author}: {issue.authorHandle}</span>
        <span>{m.collaboration.issues.assignee}: {issue.assigneeHandle || m.collaboration.issues.unassigned}</span>
        <span>{m.collaboration.created}: <time datetime={issue.createdAt}>{formatDate(issue.createdAt)}</time></span>
        <span>{m.collaboration.updated}: <time datetime={issue.updatedAt}>{formatDate(issue.updatedAt)}</time></span>
      </div>
      {#if issue.labels?.length}
        <div class="issue-label-section">
          <h2>{m.collaboration.issues.labels}</h2>
          <ul class="collaboration-tags">
            {#each issue.labels as label (label)}<li>{label}</li>{/each}
          </ul>
        </div>
      {/if}
      <section class="issue-body" aria-labelledby="issue-body-heading">
        <h2 id="issue-body-heading">{m.collaboration.details}</h2>
        <p>{issue.body || m.collaboration.emptyDescription}</p>
      </section>
      {#if issue.linkedProposalNumber && issue.linkedProposalNumber > 0}
        <p class="linked-proposal">
          {m.collaboration.issues.linkedProposal}:
          <a href={proposalDetailHref(projectId, issue.linkedProposalNumber)}>#{issue.linkedProposalNumber}</a>
        </p>
      {:else}
        <p class="meta">{m.collaboration.issues.noLinkedProposal}</p>
      {/if}
    </article>
    <CommentThread projectId={projectId} number={number} kind="issue" {locale} {m} />
  {/if}
</section>
