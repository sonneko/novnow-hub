<script lang="ts">
  import { onMount } from 'svelte';
  import { messages, type Locale } from '../../lib/i18n';
  import type { ShellPage } from '../../lib/pages';
  import {
    getChapter,
    getProject,
    placeholderChapters,
  } from '../../lib/placeholders';
  import ProjectsPage from '../pages/ProjectsPage.svelte';

  interface Props {
    page: ShellPage;
  }

  let { page }: Props = $props();
  let locale = $state<Locale>('ja');
  let theme = $state<'light' | 'dark'>('light');

  const m = $derived(messages[locale]);
  const readerHref = `/read/${placeholderChapters[0].id}`;
  const activeProject = $derived(
    page.kind === 'project' ? getProject(page.projectId) : undefined,
  );
  const activeChapter = $derived(
    page.kind === 'reader' ? getChapter(page.chapterId) : undefined,
  );

  function documentTitle(): string {
    if (page.kind === 'project') {
      return activeProject?.title[locale] ?? m.project.title;
    }
    if (page.kind === 'reader') {
      return activeChapter?.title[locale] ?? m.reader.title;
    }
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
      <a
        href={readerHref}
        aria-current={page.kind === 'reader' ? 'page' : undefined}
      >
        {m.nav.reader}
      </a>
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
      <ProjectsPage {locale} m={m} />
    {:else if page.kind === 'project'}
      <section class="section-stack" aria-labelledby="project-heading">
        <p class="breadcrumbs"><a href="/">{m.project.back}</a></p>
        <header class="page-heading">
          <h1 id="project-heading">
            {activeProject?.title[locale] ?? m.project.notFound}
          </h1>
          <p class="placeholder-note">{m.notice.placeholder} · {m.notice.noApi}</p>
        </header>
        <article class="panel">
          <h2>{m.project.overview}</h2>
          <p>{activeProject?.summary[locale] ?? m.project.notFound}</p>
        </article>
      </section>
    {:else}
      <article class="reader" aria-labelledby="reader-heading">
        <header class="reader-header">
          <p class="breadcrumbs">
            <a href={`/projects/${activeChapter?.projectId ?? ''}`}>
              {m.reader.backToProject}
            </a>
          </p>
          <h1 id="reader-heading">
            {activeChapter?.title[locale] ?? m.reader.notFound}
          </h1>
          <p class="placeholder-note">{m.reader.placeholderNote}</p>
        </header>
        <div class="reader-body">
          {#if activeChapter}
            {#each activeChapter.paragraphs as paragraph, index (index)}
              <p>{paragraph[locale]}</p>
            {/each}
          {:else}
            <p>{m.reader.notFound}</p>
          {/if}
        </div>
      </article>
    {/if}
  </main>

  <footer class="site-footer">
    <p>{m.notice.placeholder} · {m.notice.noApi}</p>
  </footer>
</div>
