<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Job } from '../../lib/api/types';
  import { inspectionPageHref, projectPageHref, derivativesPageHref } from '../../lib/links';

  interface Props {
    projectId: string;
    locale: Locale;
    m: Messages;
  }

  let { projectId, locale, m }: Props = $props();

  type JobStatus = Job['status'];

  interface DiagnosticItem {
    path: string;
    code?: string;
    message: string;
  }

  interface DiagnosticsResult {
    ok: boolean;
    errors?: DiagnosticItem[];
    warnings?: DiagnosticItem[];
    checkedAt: string;
  }

  let state = $state<'idle' | 'running' | 'done' | 'error'>('idle');
  let job = $state<Job>();
  let failure = $state<ApiFailure>();
  let running = $state(false);

  const devHeaders = import.meta.env.DEV
    ? { Authorization: 'Bearer mock-user-token' }
    : undefined;

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale === 'ja' ? 'ja-JP' : 'en-US', {
      dateStyle: 'medium',
      timeZone: 'UTC',
    }).format(date);
  }

  function statusLabel(status: JobStatus): string {
    switch (status) {
      case 'pending':
        return m.inspection.statusPending;
      case 'running':
        return m.inspection.statusRunning;
      case 'succeeded':
        return m.inspection.statusSucceeded;
      case 'failed':
        return m.inspection.statusFailed;
      default:
        return status;
    }
  }

  /// 診断ジョブを起票し、完了までポーリングする。
  async function runDiagnostics() {
    failure = undefined;
    job = undefined;
    running = true;
    state = 'running';
    try {
      const accepted = await apiClient.POST('/projects/{projectId}/diagnostics', {
        params: { path: { projectId } },
        headers: devHeaders,
      });
      if (accepted.error) {
        failure = mapApiFailure(accepted.error, accepted.response.status);
        state = 'error';
        return;
      }
      const jobId = accepted.data.jobId;
      // 完了までポーリング（最大 30 回）。
      for (let i = 0; i < 30; i += 1) {
        const result = await apiClient.GET('/jobs/{jobId}', {
          params: { path: { jobId } },
          headers: devHeaders,
        });
        if (result.error) {
          failure = mapApiFailure(result.error, result.response.status);
          state = 'error';
          return;
        }
        job = result.data;
        if (job.status === 'succeeded' || job.status === 'failed') {
          state = 'done';
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, 200));
      }
      state = 'done';
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    } finally {
      running = false;
    }
  }

  function diagnosticsResult(value: Job['result']): DiagnosticsResult | undefined {
    if (!value || typeof value !== 'object') return undefined;
    const record = value as unknown as { diagnostics?: DiagnosticsResult };
    return record.diagnostics;
  }

  let diagnostics = $derived(job ? diagnosticsResult(job.result) : undefined);

  onMount(() => {
    // 表示時に既存の最新ジョブは取得できない（一覧 API は契約に無い）ため、
    // 「点検を実行」ボタンで起票・表示する。
  });
</script>

<nav class="project-tabs" aria-label={m.project.sectionsLabel}>
  <a href={projectPageHref(projectId)}>{m.project.overviewTab}</a>
  <a href={derivativesPageHref(projectId)}>{m.project.derivativesTab}</a>
  <a href={inspectionPageHref(projectId)} aria-current="page">{m.project.inspectionTab}</a>
</nav>

<section class="panel" aria-labelledby="inspection-heading">
  <h2 id="inspection-heading">{m.inspection.title}</h2>
  <p class="lead">{m.inspection.lead}</p>
  <p>
    <button class="button button-primary" type="button" onclick={() => void runDiagnostics()} disabled={running}>
      {m.inspection.runDiagnostics}
    </button>
  </p>

  {#if state === 'error'}
    <p role="alert">{m.api.errors[failure ?? 'unknown']}</p>
  {:else if state === 'running'}
    <p class="meta">{m.inspection.running}</p>
  {:else if state === 'done' && job}
    <div class="job-card">
      <p class="meta">
        <span class="badge">{statusLabel(job.status)}</span>
        <span>{m.inspection.jobId}: {job.id}</span>
        <span>{m.inspection.kind}: {job.kind}</span>
        <span>{m.inspection.createdAt}: {formatDate(job.createdAt)}</span>
      </p>
      {#if job.error}
        <p role="alert">{m.inspection.error}: {job.error}</p>
      {/if}

      {#if job.kind === 'diagnostics' && job.status === 'succeeded'}
        {#if diagnostics}
          <h3>{m.inspection.resultTitle}</h3>
          <p>
            <span class="badge {diagnostics.ok ? 'badge-ok' : 'badge-err'}">
              {diagnostics.ok ? m.inspection.ok : m.inspection.notOk}
            </span>
            <span class="meta">{m.inspection.checkedAt}: {formatDate(diagnostics.checkedAt)}</span>
          </p>

          {#if diagnostics.errors && diagnostics.errors.length > 0}
            <h4>{m.inspection.errors} ({diagnostics.errors.length})</h4>
            <ul class="diag-list">
              {#each diagnostics.errors as err}
                <li>
                  <span class="badge">{err.code ?? 'error'}</span>
                  <span class="diag-path">{err.path || '—'}</span>
                  <span>{err.message}</span>
                </li>
              {/each}
            </ul>
          {/if}

          {#if diagnostics.warnings && diagnostics.warnings.length > 0}
            <h4>{m.inspection.warnings} ({diagnostics.warnings.length})</h4>
            <ul class="diag-list">
              {#each diagnostics.warnings as warn}
                <li>
                  <span class="badge">{warn.code ?? 'warning'}</span>
                  <span class="diag-path">{warn.path || '—'}</span>
                  <span>{warn.message}</span>
                </li>
              {/each}
            </ul>
          {/if}
        {:else}
          <p>{m.inspection.noResult}</p>
        {/if}
      {/if}
    </div>
  {/if}
</section>

<style>
  .job-card {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .lead {
    opacity: 0.85;
    margin-bottom: 1rem;
  }
  .diag-list {
    list-style: none;
    padding-left: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .diag-list li {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    flex-wrap: wrap;
  }
  .diag-path {
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
    opacity: 0.8;
  }
  .badge-ok {
    background: var(--color-ok, #e6f4ea);
    color: var(--color-ok-text, #1e6b3a);
  }
  .badge-err {
    background: var(--color-err, #fdecea);
    color: var(--color-err-text, #8c1d18);
  }
</style>
