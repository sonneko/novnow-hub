<script lang="ts">
  import type { Locale, Messages } from '../../lib/i18n';
  import { getProject, getProjectChapters } from '../../lib/placeholders';

  interface Props {
    projectId: string;
    locale: Locale;
    m: Messages;
  }

  let { projectId, locale, m }: Props = $props();

  const project = $derived(getProject(projectId));
  const chapters = $derived(getProjectChapters(projectId));
</script>

<section class="section-stack" aria-labelledby="project-heading">
  <p class="breadcrumbs"><a href="/">{m.project.back}</a></p>

  {#if project}
    <header class="page-heading">
      <div class="card-top">
        <h1 id="project-heading">{project.title[locale]}</h1>
        <span class="badge">{m.projects.status[project.status]}</span>
      </div>
      <p class="meta">
        <span>{m.projects.updated}</span>
        <time datetime={project.updatedAt}>{project.updatedAt}</time>
      </p>
      <p class="placeholder-note">{m.notice.placeholder} · {m.notice.noApi}</p>
    </header>

    <article class="panel" aria-labelledby="project-overview">
      <h2 id="project-overview">{m.project.overview}</h2>
      <p>{project.summary[locale]}</p>
    </article>

    <section class="panel" aria-labelledby="project-chapters">
      <h2 id="project-chapters">{m.project.chapters}</h2>
      {#if chapters.length === 0}
        <p>{m.project.emptyChapters}</p>
      {:else}
        <ul class="chapter-list">
          {#each chapters as chapter (chapter.id)}
            <li>
              <span>{chapter.title[locale]}</span>
              <a href={`/read/${chapter.id}`}>{m.project.read}</a>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="panel" aria-labelledby="project-activity">
      <h2 id="project-activity">{m.project.activity}</h2>
      <p>{m.project.activityPlaceholder}</p>
    </section>
  {:else}
    <article class="panel">
      <h1 id="project-heading">{m.project.notFound}</h1>
      <p><a href="/">{m.project.back}</a></p>
    </article>
  {/if}
</section>
