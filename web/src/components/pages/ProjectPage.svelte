<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Project, Tree } from '../../lib/api/types';
  import { projectPageHref, readerPageHref } from '../../lib/links';

  interface Props {
    projectId: string;
    locale: Locale;
    m: Messages;
  }

  let { projectId, locale, m }: Props = $props();
  let project = $state<Project | undefined>(undefined);
  let entries = $state<Tree['entries']>([]);
  let treeTruncated = $state(false);
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let treeFailure = $state<ApiFailure | undefined>(undefined);

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
      const treeResult = await apiClient.GET('/projects/{projectId}/tree', {
        params: { path: { projectId } },
      });

      if (treeResult.error) {
        treeFailure = mapApiFailure(treeResult.error, treeResult.response.status);
      } else {
        entries = treeResult.data.entries;
        treeTruncated = treeResult.data.truncated;
      }
      state = 'ready';
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
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
    <header class="page-heading">
      <div class="card-top">
        <h1 id="project-heading">{project.title}</h1>
        <span class="badge">{m.projects.visibility[project.visibility]}</span>
      </div>
      <p class="meta">
        <span>{m.project.owner}: {project.ownerHandle}</span>
        <span>{m.project.license}: {project.license}</span>
      </p>
      <p class="meta">
        <span>{m.project.updated}</span>
        <time datetime={project.updatedAt}>{formatDate(project.updatedAt)}</time>
      </p>
      <p><a href={readerPageHref(projectId)}>{m.project.openReader}</a></p>
    </header>

    <article class="panel" aria-labelledby="project-overview">
      <h2 id="project-overview">{m.project.overview}</h2>
      <p>{project.description || m.projects.noDescription}</p>
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
        <ul class="content-list">
          {#each entries as entry (entry.path)}
            <li>
              <span>{entry.path}</span>
              <span class="badge">
                {entry.kind === 'dir' ? m.project.directory : m.project.file}
              </span>
              <span class="meta">{entry.size} B</span>
            </li>
          {/each}
        </ul>
        {#if treeTruncated}
          <p class="meta">{m.project.truncatedContents}</p>
        {/if}
      {/if}
    </section>
  {/if}
</section>
