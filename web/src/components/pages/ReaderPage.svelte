<script lang="ts">
  import { onMount } from 'svelte';
  import type { Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { ReadingDocument } from '../../lib/api/types';
  import { projectPageHref } from '../../lib/links';
  import {
    cacheReadingDocument,
    clearCachedReadingDocument,
    getCachedReadingDocument,
  } from '../../lib/reader-cache';

  type ReaderFontSize = 'small' | 'medium' | 'large' | 'xlarge';

  interface Props {
    projectId: string;
    m: Messages;
  }

  let { projectId, m }: Props = $props();
  let document = $state<ReadingDocument | undefined>(undefined);
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let cacheState = $state<'refreshing' | 'offline' | undefined>(undefined);
  let fontSize = $state<ReaderFontSize>('medium');

  // The size is kept on this device only (localStorage), as the design requires.
  const readerFontSizes: ReaderFontSize[] = ['small', 'medium', 'large', 'xlarge'];
  const fontSizeStorageKey = 'novnow-reader-font-size';

  function isReaderFontSize(value: unknown): value is ReaderFontSize {
    return typeof value === 'string' && (readerFontSizes as string[]).includes(value);
  }

  function setFontSize(next: ReaderFontSize) {
    fontSize = next;
    try {
      localStorage.setItem(fontSizeStorageKey, next);
    } catch {
      // Storage may be unavailable; the size still applies to the current page.
    }
  }

  async function loadReadingDocument() {
    if (!projectId) {
      failure = 'not-found';
      state = 'error';
      return;
    }

    let cached: ReadingDocument | undefined;
    try {
      cached = await getCachedReadingDocument(projectId);
    } catch {
      cached = undefined;
    }
    if (cached) {
      document = cached;
      state = 'ready';
      cacheState = 'refreshing';
    }

    try {
      const result = await apiClient.GET('/projects/{projectId}/reading', {
        params: { path: { projectId } },
      });

      if (result.error) {
        const nextFailure = mapApiFailure(result.error, result.response.status);
        if (nextFailure === 'not-found') {
          // A 404 can mean a public work was unpublished; never keep serving its old body.
          await clearCachedReadingDocument(projectId);
        }
        if (cached && nextFailure !== 'not-found' && nextFailure !== 'auth-required') {
          cacheState = 'offline';
          state = 'ready';
          return;
        }
        failure = nextFailure;
        state = 'error';
        return;
      }
      if (result.data.projectId !== projectId) {
        await clearCachedReadingDocument(projectId);
        failure = 'verification-failed';
        state = 'error';
        return;
      }

      document = result.data;
      state = 'ready';
      cacheState = undefined;
      try {
        await cacheReadingDocument(result.data);
      } catch {
        // A cache write failure does not affect the live reading experience.
      }
    } catch {
      if (cached) {
        cacheState = 'offline';
        state = 'ready';
        return;
      }
      failure = 'upstream-unavailable';
      state = 'error';
    }
  }

  onMount(() => {
    try {
      const stored = localStorage.getItem(fontSizeStorageKey);
      if (isReaderFontSize(stored)) fontSize = stored;
    } catch {
      // Keep the default size when storage is unavailable.
    }
    void loadReadingDocument();
  });
</script>

<article class="reader" aria-labelledby="reader-heading" data-font-size={fontSize}>
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
    <header class="reader-header" data-project-id={document.projectId} data-etag={document.etag}>
      <p class="breadcrumbs">
        <a href={projectPageHref(projectId)}>{m.reader.backToProject}</a>
      </p>
      <h1 id="reader-heading">{document.title}</h1>
      <div class="reader-font-controls" role="group" aria-label={m.reader.fontSize.label}>
        <span class="reader-font-label" aria-hidden="true">{m.reader.fontSize.label}</span>
        {#each readerFontSizes as size (size)}
          <button
            class="button"
            type="button"
            aria-pressed={fontSize === size}
            onclick={() => setFontSize(size)}
          >
            {m.reader.fontSize[size]}
          </button>
        {/each}
      </div>
    </header>

    {#if cacheState}
      <p class="reader-cache-notice" role="status" aria-live="polite">
        {cacheState === 'refreshing' ? m.reader.cacheRefreshing : m.reader.cacheOffline}
      </p>
    {/if}

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
