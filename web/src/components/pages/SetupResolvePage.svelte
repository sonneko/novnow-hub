<script lang="ts">
  import { onMount } from 'svelte';
  import type { Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { SetupInfo } from '../../lib/api/types';
  import { projectPageHref, readerPageHref } from '../../lib/links';

  interface Props {
    setupUrl: string;
    m: Messages;
  }

  let { setupUrl: initialSetupUrl, m }: Props = $props();
  let inputUrl = $state('');
  let setupInfo = $state<SetupInfo>();
  let state = $state<'idle' | 'loading' | 'ready' | 'error'>('idle');
  let failure = $state<ApiFailure>();
  let inputMissing = $state(false);

  async function resolveSetupUrl(event?: SubmitEvent) {
    event?.preventDefault();
    const value = inputUrl.trim();
    if (!value) {
      inputMissing = true;
      return;
    }

    inputMissing = false;
    setupInfo = undefined;
    failure = undefined;
    state = 'loading';
    try {
      const currentUrl = new URL(window.location.href);
      currentUrl.searchParams.set('u', value);
      window.history.replaceState({}, '', currentUrl);

      const result = await apiClient.GET('/setup/resolve', {
        params: { query: { u: value } },
      });
      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        state = 'error';
      } else {
        setupInfo = result.data;
        state = 'ready';
      }
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    }
  }

  onMount(() => {
    if (initialSetupUrl.trim()) {
      inputUrl = initialSetupUrl;
      void resolveSetupUrl();
    }
  });
</script>

<section class="setup-resolve-page" aria-labelledby="setup-resolve-heading">
  <p class="breadcrumbs"><a href="/">{m.project.back}</a></p>
  <header class="page-heading">
    <p class="publish-eyebrow">{m.setupResolve.eyebrow}</p>
    <h1 id="setup-resolve-heading">{m.setupResolve.title}</h1>
    <p>{m.setupResolve.lead}</p>
  </header>

  <form class="panel setup-resolve-form" onsubmit={resolveSetupUrl}>
    <label for="setup-resolve-url">{m.setupResolve.inputLabel}</label>
    <input
      id="setup-resolve-url"
      bind:value={inputUrl}
      type="url"
      maxlength="2048"
      placeholder={m.setupResolve.inputPlaceholder}
      autocomplete="url"
      required
      aria-describedby="setup-resolve-help"
    />
    <p id="setup-resolve-help" class="field-hint">{m.setupResolve.inputHelp}</p>
    {#if inputMissing}
      <p class="field-error" role="alert">{m.setupResolve.inputRequired}</p>
    {/if}
    <button class="button button-primary" type="submit" disabled={state === 'loading'}>
      {state === 'loading' ? m.setupResolve.resolving : m.setupResolve.submit}
    </button>
  </form>

  {#if state === 'loading'}
    <p role="status">{m.setupResolve.resolving}</p>
  {:else if state === 'error'}
    <section class="panel setup-resolve-error" aria-labelledby="setup-resolve-error-heading">
      <h2 id="setup-resolve-error-heading">{m.setupResolve.failureTitle}</h2>
      <p class="setup-failure-stage">
        <strong>{m.setupResolve.failureStage}:</strong> {m.setupResolve.resolutionStage}
      </p>
      <p role="alert">{m.api.errors[failure ?? 'unknown']}</p>
      <button class="button" type="button" onclick={() => void resolveSetupUrl()}>
        {m.setupResolve.retry}
      </button>
    </section>
  {:else if state === 'ready' && setupInfo}
    <section class="panel setup-resolve-result" aria-labelledby="setup-result-heading">
      <div class="setup-result-heading">
        <div>
          <p class="publish-eyebrow">{m.setupResolve.eyebrow}</p>
          <h2 id="setup-result-heading">{m.setupResolve.resultTitle}</h2>
        </div>
        <span class="badge">{m.setupResolve.acquisitionKinds[setupInfo.acquisition.kind]}</span>
      </div>

      <dl class="project-meta-grid setup-result-meta">
        <div><dt>{m.setupResolve.slug}</dt><dd>{setupInfo.slug}</dd></div>
        <div><dt>{m.setupResolve.projectId}</dt><dd>{setupInfo.projectId}</dd></div>
        <div><dt>{m.setupResolve.snapshot}</dt><dd>{setupInfo.snapshotVersion}</dd></div>
        <div><dt>{m.setupResolve.minimumVersion}</dt><dd>{setupInfo.novnow.minVersion}</dd></div>
        {#if setupInfo.acquisition.ref}
          <div><dt>{m.setupResolve.ref}</dt><dd>{setupInfo.acquisition.ref}</dd></div>
        {/if}
      </dl>

      <section class="setup-steps" aria-labelledby="setup-steps-heading">
        <h3 id="setup-steps-heading">{m.setupResolve.stepsTitle}</h3>
        <ol>
          {#each m.setupResolve.steps as step, index (`${index}-${step}`)}
            <li>{step}</li>
          {/each}
        </ol>
      </section>

      <div class="setup-resolve-actions">
        <a class="button button-primary" href={readerPageHref(setupInfo.projectId)}>
          {m.setupResolve.openInBrowser}
        </a>
        <a class="button" href={projectPageHref(setupInfo.projectId)}>
          {m.project.overviewTab}
        </a>
      </div>

      <div class="setup-manual">
        <p class="setup-url-label">{m.setupResolve.acquisition}: {m.setupResolve.acquisitionKinds[setupInfo.acquisition.kind]}</p>
        <a class="setup-url" href={setupInfo.acquisition.url}>{setupInfo.acquisition.url}</a>
        <p class="meta">{m.setupResolve.manualHelp}</p>
      </div>
    </section>
  {/if}
</section>
