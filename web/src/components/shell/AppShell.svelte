<script lang="ts">
  import { onMount } from 'svelte';
  import { messages, type Locale } from '../../lib/i18n';
  import { readerPageHref } from '../../lib/links';
  import type { ShellPage } from '../../lib/pages';
  import ProjectPage from '../pages/ProjectPage.svelte';
  import ProjectsPage from '../pages/ProjectsPage.svelte';
  import ReaderPage from '../pages/ReaderPage.svelte';
  import PublishWizard from '../pages/PublishWizard.svelte';

  interface Props {
    page: ShellPage;
  }

  let { page }: Props = $props();
  let locale = $state<Locale>('ja');
  let theme = $state<'light' | 'dark'>('light');
  let projectId = $state('');
  let routeReady = $state(false);

  const m = $derived(messages[locale]);
  const readerHref = $derived(projectId ? readerPageHref(projectId) : undefined);

  function documentTitle(): string {
    if (page.kind === 'project') return m.project.title;
    if (page.kind === 'reader') return m.reader.title;
    if (page.kind === 'publish') return m.publish.title;
    return m.projects.title;
  }

  function applyLocale(next: Locale) {
    locale = next;
    document.documentElement.lang = next;
    document.title = `${documentTitle()} | ${m.app.name}`;
    try {
      localStorage.setItem('novnow-locale', next);
    } catch {
      // Storage may be unavailable; the in-memory locale still works.
    }
  }

  function toggleLocale() {
    applyLocale(locale === 'ja' ? 'en' : 'ja');
  }

  function toggleTheme() {
    theme = theme === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem('novnow-theme', theme);
    } catch {
      // Storage may be unavailable; the current page theme still works.
    }
  }

  onMount(() => {
    if (page.kind === 'project' || page.kind === 'reader') {
      const queryProjectId = new URLSearchParams(window.location.search).get('projectId');
      projectId = page.projectId?.trim() || queryProjectId?.trim() || '';
      routeReady = true;
    }

    let storedLocale: string | null = null;
    try {
      storedLocale = localStorage.getItem('novnow-locale');
    } catch {
      storedLocale = null;
    }
    applyLocale(storedLocale === 'en' ? 'en' : 'ja');
    theme = document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
  });
</script>

<a class="skip-link" href="#main-content">{m.app.skipToContent}</a>

<div class="shell">
  <header class="site-header">
    <a class="brand" href="/">{m.app.name}</a>

    <nav class="site-nav" aria-label={m.nav.primary}>
      <a href="/" aria-current={page.kind === 'projects' ? 'page' : undefined}>
        {m.nav.projects}
      </a>
      <a href="/publish/" aria-current={page.kind === 'publish' ? 'page' : undefined}>
        {m.nav.publish}
      </a>
      {#if readerHref}
        <a href={readerHref} aria-current={page.kind === 'reader' ? 'page' : undefined}>
          {m.nav.reader}
        </a>
      {/if}
    </nav>

    <div class="controls">
      <button
        type="button"
        class="control-button"
        onclick={toggleLocale}
        aria-label={m.controls.switchLanguage}
      >
        {m.controls.languageName}
      </button>
      <button
        type="button"
        class="control-button"
        onclick={toggleTheme}
        aria-label={theme === 'dark'
          ? m.controls.switchToLight
          : m.controls.switchToDark}
      >
        {theme === 'dark' ? m.controls.light : m.controls.dark}
      </button>
    </div>
  </header>

  <main id="main-content" class="content">
    {#if page.kind === 'projects'}
      <ProjectsPage {locale} {m} />
    {:else if page.kind === 'project'}
      {#if routeReady}
        <ProjectPage {projectId} {locale} {m} />
      {:else}
        <p role="status">{m.api.loading}</p>
      {/if}
    {:else if page.kind === 'reader'}
      {#if routeReady}
        <ReaderPage {projectId} {m} />
      {:else}
        <p role="status">{m.api.loading}</p>
      {/if}
    {:else}
      <PublishWizard {locale} {m} />
    {/if}
  </main>

  <footer class="site-footer">
    <p>{m.notice.mockData}</p>
  </footer>
</div>
