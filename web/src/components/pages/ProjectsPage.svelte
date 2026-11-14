<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Project } from '../../lib/api/types';
  import { projectPageHref, readerPageHref } from '../../lib/links';

  interface Props {
    locale: Locale;
    m: Messages;
  }

  let { locale, m }: Props = $props();
  let projects = $state<Project[]>([]);
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

  async function loadProjects() {
    try {
      const result = await apiClient.GET('/projects', {
        params: { query: { sort: 'updated', limit: 20 } },
      });

      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        state = 'error';
        return;
      }

      projects = result.data.items;
      state = 'ready';
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    }
  }

  onMount(() => {
    void loadProjects();
  });
</script>

<section aria-labelledby="projects-heading">
  <header class="page-heading">
    <h1 id="projects-heading">{m.projects.title}</h1>
    <p>{m.projects.lead}</p>
  </header>

  {#if state === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <p role="alert">{m.api.errors[failure]}</p>
  {:else if projects.length === 0}
    <p>{m.projects.empty}</p>
  {:else}
    <ul class="card-grid">
      {#each projects as project (project.id)}
        <li class="card">
          <div class="card-top">
            <h2>
              <a href={projectPageHref(project.id)}>{project.title}</a>
            </h2>
            <span class="badge">{m.projects.visibility[project.visibility]}</span>
          </div>
          <p>{project.description || m.projects.noDescription}</p>
          <p class="meta">
            <span>{m.projects.owner}: {project.ownerHandle}</span>
          </p>
          <p class="meta">
            <span>{m.projects.updated}</span>
            <time datetime={project.updatedAt}>{formatDate(project.updatedAt)}</time>
          </p>
          <p><a href={readerPageHref(project.id)}>{m.project.openReader}</a></p>
        </li>
      {/each}
    </ul>
  {/if}
</section>
