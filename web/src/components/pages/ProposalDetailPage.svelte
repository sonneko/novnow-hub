<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Proposal, ProposalDiff } from '../../lib/api/types';
  import { proposalListHref, projectPageHref } from '../../lib/links';

  interface Props {
    projectId: string;
    number: number;
    locale: Locale;
    m: Messages;
  }

  type Verdict = 'approve' | 'request_changes' | 'dismiss';
  let { projectId, number, locale, m }: Props = $props();
  let proposal = $state<Proposal>();
  let diff = $state<ProposalDiff>();
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let diffFailure = $state<ApiFailure>();
  let reviewFailure = $state<ApiFailure>();
  let verdict = $state<Verdict>('approve');
  let reviewBody = $state('');
  let submitting = $state(false);
  let reviewSent = $state(false);

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
      const [proposalResult, diffResult] = await Promise.allSettled([
        apiClient.GET('/projects/{projectId}/proposals/{number}', {
          params: { path: { projectId, number } },
        }),
        apiClient.GET('/projects/{projectId}/proposals/{number}/diff', {
          params: { path: { projectId, number } },
        }),
      ]);

      if (proposalResult.status === 'rejected') {
        failure = 'upstream-unavailable';
        state = 'error';
        return;
      }
      if (proposalResult.value.error) {
        failure = mapApiFailure(proposalResult.value.error, proposalResult.value.response.status);
        state = 'error';
        return;
      }
      proposal = proposalResult.value.data;

      if (diffResult.status === 'rejected') {
        diffFailure = 'upstream-unavailable';
      } else if (diffResult.value.error) {
        diffFailure = mapApiFailure(diffResult.value.error, diffResult.value.response.status);
      } else {
        diff = diffResult.value.data;
      }
      state = 'ready';
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    }
  }

  async function submitReview(event: SubmitEvent) {
    event.preventDefault();
    if (!proposal || submitting) return;
    submitting = true;
    reviewFailure = undefined;
    reviewSent = false;
    try {
      const result = await apiClient.POST('/projects/{projectId}/proposals/{number}/approve', {
        params: { path: { projectId, number } },
        headers: import.meta.env.DEV ? { Authorization: 'Bearer mock-user-token' } : undefined,
        body: { verdict, body: reviewBody.trim() || undefined },
      });
      if (result.error) {
        reviewFailure = mapApiFailure(result.error, result.response.status);
      } else {
        proposal = result.data;
        reviewSent = true;
        reviewBody = '';
      }
    } catch {
      reviewFailure = 'upstream-unavailable';
    } finally {
      submitting = false;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<section class="collaboration-page" aria-labelledby="proposal-heading">
  <p class="breadcrumbs">
    <a href={projectPageHref(projectId)}>{m.collaboration.backToProject}</a>
    <span aria-hidden="true"> / </span>
    <a href={proposalListHref(projectId)}>{m.collaboration.proposals.title}</a>
  </p>

  {#if state === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <article class="panel">
      <h1 id="proposal-heading">{m.collaboration.proposals.title}</h1>
      <p role="alert">{m.api.errors[failure]}</p>
    </article>
  {:else if proposal}
    <header class="page-heading">
      <div class="collaboration-card-top">
        <span class="collaboration-number">#{proposal.number}</span>
        <span class="badge">{m.collaboration.proposals[proposal.state]}</span>
      </div>
      <h1 id="proposal-heading">{proposal.title}</h1>
      <p class="collaboration-summary">{proposal.body || m.collaboration.emptyDescription}</p>
      <div class="collaboration-meta">
        <span>{m.collaboration.author}: {proposal.authorHandle}</span>
        <span>{m.collaboration.created}: <time datetime={proposal.createdAt}>{formatDate(proposal.createdAt)}</time></span>
        <span>{m.collaboration.proposals.source}: {proposal.baseRef}</span>
        <span>{m.collaboration.proposals.target}: {proposal.headRef}</span>
      </div>
    </header>

    <section class="panel" aria-labelledby="proposal-review-status">
      <h2 id="proposal-review-status">{m.collaboration.proposals.reviewTitle}</h2>
      <p class="review-progress">
        {m.collaboration.proposals.approvalProgress}: <strong>{proposal.approvalCount}</strong> / {proposal.requiredApprovals}
        <span>({m.collaboration.proposals.requiredApprovals})</span>
      </p>
      <h3>{m.collaboration.proposals.checks}</h3>
      {#if proposal.checks.length === 0}
        <p>{m.collaboration.proposals.diffEmpty}</p>
      {:else}
        <ul class="check-status-list">
          {#each proposal.checks as check (check.name)}
            <li>
              <span>{check.name}</span>
              <span class="badge">{m.collaboration.proposals.checkStates[check.state]}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="panel proposal-diff" aria-labelledby="proposal-diff-heading">
      <h2 id="proposal-diff-heading">{m.collaboration.proposals.diff}</h2>
      {#if diffFailure}
        <p role="alert">{m.collaboration.proposals.diffUnavailable}</p>
      {:else if diff}
        {#if diff.novnowDiagnosticsDelta}
          <div class="diagnostics-summary">
            <strong>{m.collaboration.proposals.diagnostics}</strong>
            <span>{m.collaboration.proposals.introduced}: {diff.novnowDiagnosticsDelta.introduced}</span>
            <span>{m.collaboration.proposals.resolved}: {diff.novnowDiagnosticsDelta.resolved}</span>
          </div>
        {/if}
        {#if diff.files.length === 0}
          <p>{m.collaboration.proposals.diffEmpty}</p>
        {:else}
          <ul class="proposal-file-list">
            {#each diff.files as file (file.path)}
              <li class="proposal-file panel">
                <div class="proposal-file-header">
                  <strong>{file.path}</strong>
                  <span class="badge">{m.collaboration.proposals.fileStatus[file.status]}</span>
                  <span class="diff-stat">+{file.additions ?? 0} / −{file.deletions ?? 0}</span>
                </div>
                {#each file.hunks as hunk, index (`${file.path}-${index}`)}
                  <pre class="diff-hunk"><code>{hunk}</code></pre>
                {/each}
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    </section>

    {#if proposal.state === 'open'}
      <form class="panel review-form" onsubmit={submitReview} aria-labelledby="review-form-heading">
        <h2 id="review-form-heading">{m.collaboration.proposals.reviewTitle}</h2>
        <p>{m.collaboration.proposals.reviewHelp}</p>
        <label for="review-verdict">{m.collaboration.proposals.verdict}</label>
        <select id="review-verdict" bind:value={verdict}>
          <option value="approve">{m.collaboration.proposals.approve}</option>
          <option value="request_changes">{m.collaboration.proposals.requestChanges}</option>
          <option value="dismiss">{m.collaboration.proposals.dismiss}</option>
        </select>
        <label for="review-body">{m.collaboration.proposals.reviewNote}</label>
        <textarea id="review-body" bind:value={reviewBody} maxlength="10000" rows="4" placeholder={m.collaboration.proposals.reviewPlaceholder}></textarea>
        {#if reviewFailure}
          <p role="alert">{m.api.errors[reviewFailure]}</p>
        {/if}
        {#if reviewSent}
          <p class="review-success" role="status">{m.collaboration.proposals.reviewSent}</p>
        {/if}
        <button class="button button-primary" type="submit" disabled={submitting}>
          {submitting ? m.collaboration.proposals.submitting : m.collaboration.proposals.submitReview}
        </button>
      </form>
    {/if}
  {/if}
</section>
