<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { FileContent, Project, SetupInfo, Tree } from '../../lib/api/types';
  import { projectPageHref, readerPageHref } from '../../lib/links';

  interface Props {
    projectId: string;
    locale: Locale;
    m: Messages;
  }

  let { projectId, locale, m }: Props = $props();
  let project = $state<Project>();
  let entries = $state<Tree['entries']>([]);
  let setupInfo = $state<SetupInfo>();
  let treeTruncated = $state(false);
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let treeFailure = $state<ApiFailure>();
  let setupFailure = $state<ApiFailure>();
  let selectedPath = $state('');
  let fileContent = $state<FileContent>();
  let fileState = $state<'idle' | 'loading' | 'ready' | 'error'>('idle');
  let fileFailure = $state<ApiFailure>('unknown');
  let copyState = $state<'idle' | 'copied' | 'failed'>('idle');

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale === 'ja' ? 'ja-JP' : 'en-US', {
      dateStyle: 'medium',
      timeZone: 'UTC',
    }).format(date);
  }

  async function loadProject() {
    if (!projectId) {
      failure = 'not-found';
      state = 'error';
      return;
    }

    try {
      const projectResult = await apiClient.GET('/projects/{projectId}', {
        params: { path: { projectId } },
      });

      if (projectResult.error) {
        failure = mapApiFailure(projectResult.error, projectResult.response.status);
        state = 'error';
        return;
      }

      project = projectResult.data;
      const [treeResult, setupResult] = await Promise.allSettled([
        apiClient.GET('/projects/{projectId}/tree', { params: { path: { projectId } } }),
        apiClient.GET('/projects/{projectId}/setup', { params: { path: { projectId } } }),
      ]);

      if (treeResult.status === 'fulfilled') {
        if (treeResult.value.error) {
          treeFailure = mapApiFailure(treeResult.value.error, treeResult.value.response.status);
        } else {
          entries = treeResult.value.data.entries;
          treeTruncated = treeResult.value.data.truncated;
        }
      } else {
        treeFailure = 'upstream-unavailable';
      }

      if (setupResult.status === 'fulfilled') {
        if (setupResult.value.error) {
          setupFailure = mapApiFailure(setupResult.value.error, setupResult.value.response.status);
        } else {
          setupInfo = setupResult.value.data;
        }
      } else {
        setupFailure = 'upstream-unavailable';
      }
      state = 'ready';
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    }
  }

  async function openFile(path: string) {
    selectedPath = path;
    fileContent = undefined;
    fileState = 'loading';
    try {
      const result = await apiClient.GET('/projects/{projectId}/file', {
        params: { path: { projectId }, query: { path } },
      });
      if (result.error) {
        fileFailure = mapApiFailure(result.error, result.response.status);
        fileState = 'error';
      } else {
        fileContent = result.data;
        fileState = 'ready';
      }
    } catch {
      fileFailure = 'upstream-unavailable';
      fileState = 'error';
    }
  }

  function closeFilePreview() {
    selectedPath = '';
    fileContent = undefined;
    fileState = 'idle';
  }

  async function copySetupUrl() {
    if (!setupInfo) return;
    try {
      await navigator.clipboard.writeText(setupInfo.acquisition.url);
      copyState = 'copied';
    } catch {
      copyState = 'failed';
    }
  }

  onMount(() => {
    void loadProject();
  });
</script>

<section class="section-stack" aria-labelledby="project-heading">
  <p class="breadcrumbs"><a href="/">{m.project.back}</a></p>

  {#if state === 'loading'}
    <header class="page-heading">
      <h1 id="project-heading">{m.project.title}</h1>
    </header>
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <article class="panel">
      <h1 id="project-heading">
        {failure === 'not-found' ? m.project.notFound : m.project.title}
      </h1>
      <p role="alert">{m.api.errors[failure]}</p>
      <p><a href="/">{m.project.back}</a></p>
    </article>
  {:else if project}
    <header class="page-heading project-detail-heading">
      <div class="card-top">
        <h1 id="project-heading">{project.title}</h1>
        <span class="badge">{m.projects.visibility[project.visibility]}</span>
      </div>
      <p class="project-detail-description">{project.description || m.projects.noDescription}</p>
      <dl class="project-meta-grid">
        <div><dt>{m.project.owner}</dt><dd>{project.ownerHandle}</dd></div>
        <div><dt>{m.project.license}</dt><dd>{project.license}</dd></div>
        <div><dt>{m.project.updated}</dt><dd><time datetime={project.updatedAt}>{formatDate(project.updatedAt)}</time></dd></div>
        <div><dt>{m.project.derivativeCount}</dt><dd>{project.derivativeCount}</dd></div>
      </dl>
      <p><a class="button button-primary" href={readerPageHref(projectId)}>{m.project.openReader}</a></p>
    </header>

    <article class="panel" aria-labelledby="project-overview">
      <h2 id="project-overview">{m.project.overview}</h2>
      {#if project.tags?.length}
        <p class="meta">
          <span>{m.project.tags}: {project.tags.join(', ')}</span>
        </p>
      {/if}
    </article>

    <section class="panel" aria-labelledby="project-contents">
      <h2 id="project-contents">{m.project.contents}</h2>
      {#if treeFailure}
        <p role="alert">{m.api.errors[treeFailure]}</p>
      {:else if entries.length === 0}
        <p>{m.project.emptyContents}</p>
      {:else}
        <ul class="content-list project-tree">
          {#each entries as entry (entry.path)}
            <li>
              <span class="project-tree-path">{entry.path}</span>
              <span class="badge">
                {entry.kind === 'dir' ? m.project.directory : m.project.file}
              </span>
              <span class="meta">{entry.size} B</span>
              {#if entry.kind === 'file'}
                <button class="button project-file-button" type="button" onclick={() => void openFile(entry.path)}>
                  {m.project.previewFile}
                </button>
              {/if}
            </li>
          {/each}
        </ul>
        {#if treeTruncated}
          <p class="meta">{m.project.truncatedContents}</p>
        {/if}
      {/if}

      {#if selectedPath}
        <section class="file-preview" aria-labelledby="file-preview-heading" aria-live="polite">
          <div class="file-preview-header">
            <h3 id="file-preview-heading">{m.project.filePreview}: {selectedPath}</h3>
            <button class="button" type="button" onclick={closeFilePreview}>{m.project.closePreview}</button>
          </div>
          {#if fileState === 'loading'}
            <p role="status">{m.project.fileLoading}</p>
          {:else if fileState === 'error'}
            <p role="alert">{m.api.errors[fileFailure]}</p>
          {:else if fileContent}
            <p class="meta">{m.project.fileEncoding}: {fileContent.encoding}</p>
            <pre class="file-preview-content">{fileContent.content ?? fileContent.data ?? ''}</pre>
          {/if}
        </section>
      {/if}
    </section>

    <section class="panel setup-panel" aria-labelledby="project-setup-heading">
      <h2 id="project-setup-heading">{m.project.setupTitle}</h2>
      {#if setupFailure}
        <p role="alert">{m.project.setupUnavailable}</p>
      {:else if setupInfo}
        <p>{m.project.setupLead}</p>
        <p class="setup-url-label">{m.project.setupUrl}</p>
        <a class="setup-url" href={setupInfo.acquisition.url}>{setupInfo.acquisition.url}</a>
        <button class="button" type="button" onclick={copySetupUrl}>
          {copyState === 'copied' ? m.project.copied : m.project.copySetupUrl}
        </button>
        {#if copyState === 'failed'}
          <p role="status">{m.project.copyFailed}</p>
        {/if}
        <dl class="project-meta-grid setup-meta">
          <div><dt>{m.project.setupSnapshot}</dt><dd>{setupInfo.snapshotVersion}</dd></div>
          <div><dt>{m.project.setupMinVersion}</dt><dd>{setupInfo.novnow.minVersion}</dd></div>
        </dl>
      {/if}
    </section>
  {/if}
</section>
