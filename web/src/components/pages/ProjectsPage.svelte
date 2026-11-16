<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { Project } from '../../lib/api/types';
  import { ProjectIndex } from '../../lib/project-index';
  import { projectPageHref, readerPageHref } from '../../lib/links';

  interface Props {
    locale: Locale;
    m: Messages;
  }

  type SearchSort = 'relevance' | 'updated';
  type ListSort = 'updated' | 'created' | 'derivatives';
  const pageSize = 12;
  const projectIndex = new ProjectIndex();

  let { locale, m }: Props = $props();
  let projects = $state<Project[]>([]);
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let keywordInput = $state('');
  let activeQuery = $state('');
  let licenseInput = $state('');
  let activeLicense = $state<Project['license']>();
  let tagInput = $state('');
  let activeTag = $state('');
  let searchSort = $state<SearchSort>('relevance');
  let listSort = $state<ListSort>('updated');
  let nextCursor = $state<string | null>(null);
  let loadingMore = $state(false);
  let requestVersion = 0;

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale === 'ja' ? 'ja-JP' : 'en-US', {
      dateStyle: 'medium',
      timeZone: 'UTC',
    }).format(date);
  }

  async function loadPage(cursor?: string) {
    const version = requestVersion;
    if (cursor) loadingMore = true;
    else state = 'loading';

    try {
      const result = activeQuery
        ? await apiClient.GET('/search/projects', {
            params: {
              query: {
                q: activeQuery,
                license: activeLicense,
                tag: activeTag || undefined,
                sort: searchSort,
                cursor,
                limit: pageSize,
              },
            },
          })
        : await apiClient.GET('/projects', {
            params: { query: { sort: listSort, cursor, limit: pageSize } },
          });

      if (version !== requestVersion) return;
      if (result.error) {
        failure = mapApiFailure(result.error, result.response.status);
        state = 'error';
        return;
      }

      projectIndex.add(result.data.items);
      projects = projectIndex.search({ license: activeLicense, tag: activeTag });
      nextCursor = result.data.nextCursor;
      state = 'ready';
    } catch {
      if (version !== requestVersion) return;
      failure = 'upstream-unavailable';
      state = 'error';
    } finally {
      if (version === requestVersion) loadingMore = false;
    }
  }

  function submitSearch(event: SubmitEvent) {
    event.preventDefault();
    activeQuery = keywordInput.trim();
    activeLicense = licenseInput ? (licenseInput as Project['license']) : undefined;
    activeTag = tagInput.trim();
    projectIndex.clear();
    projects = [];
    nextCursor = null;
    loadingMore = false;
    failure = 'unknown';
    requestVersion += 1;
    void loadPage();
  }

  function loadMore() {
    if (nextCursor && !loadingMore) void loadPage(nextCursor);
  }

  onMount(() => {
    void loadPage();
  });
</script>

<section aria-labelledby="projects-heading">
  <header class="page-heading">
    <h1 id="projects-heading">{m.projects.title}</h1>
    <p>{m.projects.lead}</p>
  </header>

  <form class="project-search panel" onsubmit={submitSearch} aria-label={m.projects.search.formLabel}>
    <div class="project-search-keyword">
      <label for="project-keyword">{m.projects.search.keyword}</label>
      <input
        id="project-keyword"
        type="search"
        bind:value={keywordInput}
        maxlength="200"
        placeholder={m.projects.search.keywordPlaceholder}
      />
    </div>
    <div class="project-search-filters">
      <div>
        <label for="project-license">{m.projects.search.license}</label>
        <select id="project-license" bind:value={licenseInput}>
          <option value="">{m.projects.search.anyLicense}</option>
          <option value="cc0">CC0</option>
          <option value="cc-by">CC BY</option>
          <option value="cc-by-sa">CC BY-SA</option>
          <option value="cc-by-nc">CC BY-NC</option>
          <option value="custom">{m.projects.search.customLicense}</option>
        </select>
      </div>
      <div>
        <label for="project-tag">{m.projects.search.tag}</label>
        <input id="project-tag" type="search" bind:value={tagInput} maxlength="50" />
      </div>
      <div>
        <label for="project-sort">{m.projects.search.sort}</label>
        {#if keywordInput.trim()}
          <select id="project-sort" bind:value={searchSort}>
            <option value="relevance">{m.projects.search.sortOptions.relevance}</option>
            <option value="updated">{m.projects.search.sortOptions.updated}</option>
          </select>
        {:else}
          <select id="project-sort" bind:value={listSort}>
            <option value="updated">{m.projects.search.sortOptions.updated}</option>
            <option value="created">{m.projects.search.sortOptions.created}</option>
            <option value="derivatives">{m.projects.search.sortOptions.derivatives}</option>
          </select>
        {/if}
      </div>
      <button class="button button-primary" type="submit">{m.projects.search.submit}</button>
    </div>
    <p class="field-hint">{m.projects.search.hint}</p>
  </form>

  {#if state === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <p role="alert">{m.api.errors[failure]}</p>
  {:else if projects.length === 0}
    <p class="project-search-empty">{activeQuery || activeLicense || activeTag ? m.projects.search.noResults : m.projects.empty}</p>
  {:else}
    <p class="project-result-count" aria-live="polite">{m.projects.search.resultCount(projects.length)}</p>
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
          {#if project.tags?.length}
            <ul class="project-tags" aria-label={m.project.tags}>
              {#each project.tags as tag (tag)}<li>{tag}</li>{/each}
            </ul>
          {/if}
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

  {#if state === 'ready' && nextCursor}
    <div class="project-pagination">
      <button class="button" type="button" onclick={loadMore} disabled={loadingMore}>
        {loadingMore ? m.projects.search.loadingMore : m.projects.search.loadMore}
      </button>
    </div>
  {/if}
</section>
