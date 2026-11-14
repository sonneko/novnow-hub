<script lang="ts">
  import type { Locale, Messages } from '../../lib/i18n';
  import { placeholderProjects } from '../../lib/placeholders';

  interface Props {
    locale: Locale;
    m: Messages;
  }

  let { locale, m }: Props = $props();
</script>

<section aria-labelledby="projects-heading">
  <header class="page-heading">
    <h1 id="projects-heading">{m.projects.title}</h1>
    <p>{m.projects.lead}</p>
  </header>

  {#if placeholderProjects.length === 0}
    <p>{m.projects.empty}</p>
  {:else}
    <ul class="card-grid">
      {#each placeholderProjects as project (project.id)}
        <li class="card">
          <div class="card-top">
            <h2>
              <a href={`/projects/${project.id}`}>{project.title[locale]}</a>
            </h2>
            <span class="badge">{m.projects.status[project.status]}</span>
          </div>
          <p>{project.summary[locale]}</p>
          <p class="meta">
            <span>{m.projects.updated}</span>
            <time datetime={project.updatedAt}>{project.updatedAt}</time>
          </p>
        </li>
      {/each}
    </ul>
  {/if}
</section>
