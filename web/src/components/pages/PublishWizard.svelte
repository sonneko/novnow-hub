<script lang="ts">
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Project } from '../../lib/api/types';
  import { projectPageHref } from '../../lib/links';

  interface Props {
    locale: Locale;
    m: Messages;
  }

  let { locale, m }: Props = $props();
  let step = $state(1);
  let file = $state<File>();
  let title = $state('');
  let description = $state('');
  let license = $state<'cc0' | 'cc-by' | 'cc-by-sa' | 'cc-by-nc' | 'custom'>('cc-by');
  let visibility = $state<'public' | 'private'>('public');
  let submitting = $state(false);
  let idempotencyKey = $state('');
  let failure = $state<ApiFailure>();
  let published = $state<Project>();
  let fileError = $state(false);

  const labels = $derived(m.publish.steps);

  function selectFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    file = input.files?.[0];
    fileError = Boolean(file && (!file.name.toLowerCase().endsWith('.zip') || file.size > 50 * 1024 * 1024));
  }

  function canContinue(): boolean {
    if (step === 1) return Boolean(file && !fileError);
    if (step === 2) return title.trim().length > 0;
    return true;
  }

  function nextStep() {
    if (canContinue() && step < 4) step += 1;
  }

  function previousStep() {
    if (step === 4) idempotencyKey = '';
    if (step > 1) step -= 1;
  }

  async function publishProject() {
    if (!file || !canContinue() || submitting) return;
    submitting = true;
    failure = undefined;

    try {
      if (!idempotencyKey) {
        idempotencyKey = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;
      }
      const result = await apiClient.POST('/projects', {
        params: { header: { 'Idempotency-Key': idempotencyKey } },
        // Prism enforces the contract's bearer security even in local mock mode.
        headers: import.meta.env.DEV ? { Authorization: 'Bearer mock-user-token' } : undefined,
        body: {
          title: title.trim(),
          description: description.trim(),
          license,
          visibility,
          // The OpenAPI binary field is represented as string in generated TypeScript.
          archive: file as unknown as string,
        },
        bodySerializer: (body) => {
          const form = new FormData();
          form.set('title', body.title);
          form.set('description', body.description ?? '');
          form.set('license', body.license);
          form.set('visibility', body.visibility ?? 'public');
          form.set('archive', file, file.name);
          return form;
        },
      });

      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
      } else {
        published = result.data;
      }
    } catch {
      failure = 'upstream-unavailable';
    } finally {
      submitting = false;
    }
  }

  function licenseLabel(value: typeof license): string {
    return m.publish.licenses[value];
  }
</script>

{#if published}
  <section class="publish-complete" aria-labelledby="publish-complete-heading">
    <div class="publish-success-mark" aria-hidden="true">✓</div>
    <p class="publish-eyebrow">{m.publish.completeEyebrow}</p>
    <h1 id="publish-complete-heading">{m.publish.completeTitle}</h1>
    <p>{m.publish.completeLead}</p>
    <div class="panel publish-url-panel">
      <h2>{published.title}</h2>
      <p>{m.publish.publicUrl}</p>
      <a class="publish-url" href={published.publishedUrl}>{published.publishedUrl}</a>
      <div class="publish-complete-actions">
        <a class="button button-primary" href={published.publishedUrl}>{m.publish.openPublished}</a>
        <a class="button" href={projectPageHref(published.id)}>{m.publish.viewHubPage}</a>
      </div>
    </div>
    <p class="publish-next-hint">{m.publish.completeHint}</p>
  </section>
{:else}
  <section class="publish-wizard" aria-labelledby="publish-heading">
    <header class="page-heading">
      <p class="publish-eyebrow">{m.publish.eyebrow}</p>
      <h1 id="publish-heading">{m.publish.title}</h1>
      <p>{m.publish.lead}</p>
    </header>

    <ol class="publish-progress" aria-label={m.publish.progressLabel}>
      {#each labels as label, index}
        <li class:current={step === index + 1} class:complete={step > index + 1}>
          <span class="publish-step-number" aria-hidden="true">{step > index + 1 ? '✓' : index + 1}</span>
          <span>{label}</span>
        </li>
      {/each}
    </ol>

    <div class="panel publish-step-panel" aria-live="polite">
      {#if step === 1}
        <h2>{m.publish.stepsContent.fileTitle}</h2>
        <p>{m.publish.stepsContent.fileHelp}</p>
        <label class="publish-file-picker">
          <span class="publish-file-icon" aria-hidden="true">↑</span>
          <span>{file ? file.name : m.publish.chooseFile}</span>
          <span class="meta">{m.publish.fileConstraints}</span>
          <input type="file" accept=".zip,application/zip" onchange={selectFile} />
        </label>
        {#if fileError}
          <p class="field-error" role="alert">{m.publish.fileError}</p>
        {/if}
      {:else if step === 2}
        <h2>{m.publish.stepsContent.infoTitle}</h2>
        <p>{m.publish.stepsContent.infoHelp}</p>
        <div class="publish-fields">
          <label for="publish-title">{m.publish.fields.title}<span aria-hidden="true"> *</span></label>
          <input id="publish-title" bind:value={title} maxlength="120" required />
          <span class="field-hint">{m.publish.fields.titleHint}</span>
          <label for="publish-description">{m.publish.fields.description}</label>
          <textarea id="publish-description" bind:value={description} maxlength="500" rows="4"></textarea>
          <span class="field-hint">{m.publish.fields.descriptionHint}</span>
        </div>
      {:else if step === 3}
        <h2>{m.publish.stepsContent.termsTitle}</h2>
        <p>{m.publish.stepsContent.termsHelp}</p>
        <fieldset class="publish-choice-group">
          <legend>{m.publish.fields.license}</legend>
          {#each Object.entries(m.publish.licenses) as [value, label]}
            <label class="publish-choice">
              <input type="radio" name="license" value={value} checked={license === value} onchange={() => license = value as typeof license} />
              <span>{label}</span>
            </label>
          {/each}
        </fieldset>
        <fieldset class="publish-choice-group">
          <legend>{m.publish.fields.visibility}</legend>
          <label class="publish-choice">
            <input type="radio" name="visibility" value="public" checked={visibility === 'public'} onchange={() => visibility = 'public'} />
            <span><strong>{m.publish.visibility.public}</strong><small>{m.publish.visibility.publicHint}</small></span>
          </label>
          <label class="publish-choice">
            <input type="radio" name="visibility" value="private" checked={visibility === 'private'} onchange={() => visibility = 'private'} />
            <span><strong>{m.publish.visibility.private}</strong><small>{m.publish.visibility.privateHint}</small></span>
          </label>
        </fieldset>
      {:else}
        <h2>{m.publish.stepsContent.reviewTitle}</h2>
        <p>{m.publish.stepsContent.reviewHelp}</p>
        <dl class="publish-review">
          <div><dt>{m.publish.fields.archive}</dt><dd>{file?.name}</dd></div>
          <div><dt>{m.publish.fields.title}</dt><dd>{title}</dd></div>
          <div><dt>{m.publish.fields.description}</dt><dd>{description || m.publish.noDescription}</dd></div>
          <div><dt>{m.publish.fields.license}</dt><dd>{licenseLabel(license)}</dd></div>
          <div><dt>{m.publish.fields.visibility}</dt><dd>{m.publish.visibility[visibility]}</dd></div>
        </dl>
        <p class="publish-confirm-note">{m.publish.confirmNote}</p>
      {/if}

      {#if failure}
        <p class="field-error" role="alert">{m.api.errors[failure]}</p>
      {/if}

      <div class="publish-actions">
        {#if step > 1}
          <button type="button" class="button" onclick={previousStep} disabled={submitting}>{m.publish.back}</button>
        {:else}
          <a class="button" href="/">{m.publish.cancel}</a>
        {/if}
        {#if step < 4}
          <button type="button" class="button button-primary" onclick={nextStep} disabled={!canContinue()}>{m.publish.next}</button>
        {:else}
          <button type="button" class="button button-primary" onclick={publishProject} disabled={submitting}>
            {submitting ? m.publish.submitting : m.publish.publishNow}
          </button>
        {/if}
      </div>
    </div>
  </section>
{/if}
