<script lang="ts">
  import { onMount } from 'svelte';
  import type { Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { ReadingDocument } from '../../lib/api/types';
  import { projectPageHref } from '../../lib/links';

  interface Props {
    projectId: string;
    m: Messages;
  }

  let { projectId, m }: Props = $props();
  let document = $state<ReadingDocument | undefined>(undefined);
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');

  async function loadReadingDocument() {
    if (!projectId) {
      failure = 'not-found';
      state = 'error';
      return;
    }

    try {
      const result = await apiClient.GET('/projects/{projectId}/reading', {
        params: { path: { projectId } },
      });

      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        state = 'error';
        return;
      }

      document = result.data;
      state = 'ready';
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    }
  }

  onMount(() => {
    void loadReadingDocument();
  });
</script>

<article class="reader" aria-labelledby="reader-heading">
  {#if state === 'loading'}
    <header class="reader-header">
      <h1 id="reader-heading">{m.reader.title}</h1>
    </header>
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <header class="reader-header">
      <h1 id="reader-heading">
        {failure === 'not-found' ? m.api.errors['not-found'] : m.reader.title}
      </h1>
    </header>
    <p role="alert">{m.api.errors[failure]}</p>
    <p><a href={projectPageHref(projectId)}>{m.reader.backToProject}</a></p>
  {:else if document}
    <header class="reader-header">
      <p class="breadcrumbs">
        <a href={projectPageHref(projectId)}>{m.reader.backToProject}</a>
      </p>
      <h1 id="reader-heading">{document.title}</h1>
    </header>

    <div class="reader-body">
      {#if document.blocks.length === 0}
        <p>{m.reader.empty}</p>
      {:else}
        {#each document.blocks as block, index (`${index}-${block.kind}`)}
          {#if block.kind === 'heading'}
            <h2>{block.text}</h2>
          {:else if block.kind === 'dialogue'}
            <blockquote>
              <p>{block.text}</p>
              {#if block.speaker}
                <cite>{block.speaker}</cite>
              {/if}
            </blockquote>
          {:else if block.kind === 'scene_break'}
            <div class="scene-break" role="separator" aria-label={m.reader.sceneBreak}>
              {block.text}
            </div>
          {:else}
            <p>{block.text}</p>
          {/if}
        {/each}
      {/if}
    </div>
  {/if}
</article>
