<script lang="ts">
  import type { Locale, Messages } from '../../lib/i18n';
  import {
    getChapter,
    getProject,
    getProjectChapters,
  } from '../../lib/placeholders';

  interface Props {
    chapterId: string;
    locale: Locale;
    m: Messages;
  }

  let { chapterId, locale, m }: Props = $props();

  const chapter = $derived(getChapter(chapterId));
  const project = $derived(chapter ? getProject(chapter.projectId) : undefined);
  const siblings = $derived(chapter ? getProjectChapters(chapter.projectId) : []);
  const chapterIndex = $derived(
    chapter ? siblings.findIndex((item) => item.id === chapter.id) : -1,
  );
  const previous = $derived(chapterIndex > 0 ? siblings[chapterIndex - 1] : undefined);
  const next = $derived(
    chapterIndex >= 0 && chapterIndex < siblings.length - 1
      ? siblings[chapterIndex + 1]
      : undefined,
  );
</script>

<article class="reader" aria-labelledby="reader-heading">
  {#if chapter}
    <header class="reader-header">
      <p class="breadcrumbs">
        {#if project}
          <a href={`/projects/${project.id}`}>{m.reader.backToProject}</a>
        {:else}
          <a href="/">{m.reader.backToProject}</a>
        {/if}
      </p>
      <h1 id="reader-heading">{chapter.title[locale]}</h1>
      {#if project}
        <p class="meta">{project.title[locale]}</p>
      {/if}
      <p class="placeholder-note">{m.reader.placeholderNote}</p>
    </header>

    <div class="reader-body">
      {#each chapter.paragraphs as paragraph, index (index)}
        <p>{paragraph[locale]}</p>
      {/each}
    </div>

    <nav class="reader-nav" aria-label={m.reader.navigation}>
      {#if previous}
        <a rel="prev" href={`/read/${previous.id}`}>← {m.reader.previous}</a>
      {:else}
        <span class="reader-nav-disabled" aria-disabled="true">
          ← {m.reader.previous}
        </span>
      {/if}

      {#if next}
        <a rel="next" href={`/read/${next.id}`}>{m.reader.next} →</a>
      {:else}
        <span class="reader-nav-disabled" aria-disabled="true">
          {m.reader.next} →
        </span>
      {/if}
    </nav>
  {:else}
    <header class="reader-header">
      <h1 id="reader-heading">{m.reader.notFound}</h1>
    </header>
    <p><a href="/">{m.project.back}</a></p>
  {/if}
</article>
