<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { AutomergeState, Proposal, ProposalDiff } from '../../lib/api/types';
  import { proposalListHref, projectPageHref } from '../../lib/links';
  import CommentThread from './CommentThread.svelte';

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
  let automerge = $state<AutomergeState>();
  let automergeFailure = $state<ApiFailure>();
  let automergeNotice = $state<'reserved' | 'canceled'>();
  let automergeApprovals = $state(1);
  let automergeSubmitting = $state(false);

  const automergeStates = $derived(m.collaboration.proposals.automerge.states);
  /** 予約できる状態: 未予約（none）か取消後（canceled）か失敗後（failed）。 */
  const canReserve = $derived(automerge !== undefined && (automerge.state === 'none' || automerge.state === 'canceled' || automerge.state === 'failed'));
  /** 取消できる状態: 予約済みか評価中。 */
  const canCancel = $derived(automerge !== undefined && (automerge.state === 'reserved' || automerge.state === 'armed'));

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
      const [proposalResult, diffResult, automergeResult] = await Promise.allSettled([
        apiClient.GET('/projects/{projectId}/proposals/{number}', {
          params: { path: { projectId, number } },
        }),
        apiClient.GET('/projects/{projectId}/proposals/{number}/diff', {
          params: { path: { projectId, number } },
        }),
        apiClient.GET('/projects/{projectId}/proposals/{number}/automerge', {
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

      // 自動マージ状態は表示補助なので、失敗しても提案本体の表示は止めない。
      if (automergeResult.status === 'rejected') {
        automergeFailure = 'upstream-unavailable';
      } else if (automergeResult.value.error) {
        automergeFailure = mapApiFailure(automergeResult.value.error, automergeResult.value.response.status);
      } else {
        automerge = automergeResult.value.data;
        automergeApprovals = automerge.requiredApprovals ?? 1;
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

  async function reserveAutomerge(event: SubmitEvent) {
    event.preventDefault();
    if (automergeSubmitting) return;
    automergeSubmitting = true;
    automergeFailure = undefined;
    automergeNotice = undefined;
    try {
      const result = await apiClient.PUT('/projects/{projectId}/proposals/{number}/automerge', {
        params: { path: { projectId, number } },
        headers: automerge
          ? (import.meta.env.DEV ? { Authorization: 'Bearer mock-user-token', 'If-Match': automerge.etag ?? undefined } : { 'If-Match': automerge.etag ?? undefined })
          : import.meta.env.DEV
            ? { Authorization: 'Bearer mock-user-token' }
            : undefined,
        body: {
          requiredApprovals: Math.max(0, Math.min(10, Math.floor(Number(automergeApprovals) || 1))),
        },
      });
      if (result.error) {
        automergeFailure = mapApiFailure(result.error, result.response.status);
      } else if (result.data) {
        automerge = result.data;
        automergeNotice = 'reserved';
      }
    } catch {
      automergeFailure = 'upstream-unavailable';
    } finally {
      automergeSubmitting = false;
    }
  }

  async function cancelAutomerge() {
    if (automergeSubmitting) return;
    automergeSubmitting = true;
    automergeFailure = undefined;
    automergeNotice = undefined;
    try {
      const result = await apiClient.DELETE('/projects/{projectId}/proposals/{number}/automerge', {
        params: { path: { projectId, number } },
        headers: import.meta.env.DEV ? { Authorization: 'Bearer mock-user-token' } : undefined,
      });
      if (result.error) {
        automergeFailure = mapApiFailure(result.error, result.response.status);
      } else {
        const refreshed = await apiClient.GET('/projects/{projectId}/proposals/{number}/automerge', {
          params: { path: { projectId, number } },
        });
        if (refreshed.error) {
          // 再取得失敗時は notice を立てず、失敗のみを表示する。
          automergeFailure = mapApiFailure(refreshed.error, refreshed.response.status);
        } else {
          automerge = refreshed.data;
          automergeNotice = 'canceled';
        }
      }
    } catch {
      automergeFailure = 'upstream-unavailable';
    } finally {
      automergeSubmitting = false;
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

    <section class="panel automerge-panel" aria-labelledby="proposal-automerge-heading">
      <h2 id="proposal-automerge-heading">{m.collaboration.proposals.automerge.title}</h2>
      <p class="automerge-help">{m.collaboration.proposals.automerge.help}</p>
      {#if automerge}
        <p class="automerge-status">
          {m.collaboration.proposals.automerge.statusLabel}:
          <span class="badge">{automergeStates[automerge.state]}</span>
        </p>
        {#if automerge.state === 'failed' && automerge.reason}
          <p class="automerge-reason" role="alert">
            {m.collaboration.proposals.automerge.failedReason}: {automerge.reason}
          </p>
        {/if}
        <div class="automerge-conditions">
          <h3>{m.collaboration.proposals.automerge.conditions}</h3>
          <ul>
            <li>
              {m.collaboration.proposals.automerge.requiredApprovals}:
              <strong>{automerge.requiredApprovals ?? 1}</strong>
            </li>
            <li>
              {m.collaboration.proposals.automerge.requiredChecks}:
              {#if automerge.requiredChecks && automerge.requiredChecks.length > 0}
                <ul class="automerge-check-list">
                  {#each automerge.requiredChecks as check (check)}<li>{check}</li>{/each}
                </ul>
              {:else}0 件{/if}
            </li>
            <li>
              {m.collaboration.proposals.automerge.requireNoConflicts}:
              <span class="badge">
                {automerge.requireNoConflicts ? m.collaboration.proposals.automerge.conflictsRequired : m.collaboration.proposals.automerge.conflictsNotRequired}
              </span>
            </li>
          </ul>
        </div>
      {:else if automergeFailure}
        <p role="alert">{m.api.errors[automergeFailure]}</p>
      {:else}
        <p role="status">{m.api.loading}</p>
      {/if}

      {#if canReserve}
        <form class="automerge-form" onsubmit={reserveAutomerge}>
          <label for="automerge-approvals">{m.collaboration.proposals.automerge.approvalsLabel}</label>
          <input
            id="automerge-approvals"
            type="number"
            min="0"
            max="10"
            step="1"
            bind:value={automergeApprovals}
          />
          <button class="button button-primary" type="submit" disabled={automergeSubmitting}>
            {automergeSubmitting ? m.collaboration.proposals.submitting : m.collaboration.proposals.automerge.reserve}
          </button>
        </form>
      {/if}

      {#if canCancel}
        <button class="button button-secondary" type="button" onclick={cancelAutomerge} disabled={automergeSubmitting}>
          {automergeSubmitting ? m.collaboration.proposals.submitting : m.collaboration.proposals.automerge.cancel}
        </button>
      {/if}

      {#if automergeNotice === 'reserved'}
        <p class="review-success" role="status">{m.collaboration.proposals.automerge.reserved}</p>
      {/if}
      {#if automergeNotice === 'canceled'}
        <p class="review-success" role="status">{m.collaboration.proposals.automerge.canceled}</p>
      {/if}
      {#if automergeFailure}
        <p role="alert">{m.api.errors[automergeFailure]}</p>
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

    <CommentThread projectId={projectId} number={number} kind="proposal" {locale} {m} />

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
