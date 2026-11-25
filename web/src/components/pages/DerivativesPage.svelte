<script lang="ts">
  import { onMount } from 'svelte';
  import type { Locale, Messages } from '../../lib/i18n';
  import { apiClient } from '../../lib/api/client';
  import { mapApiFailure, type ApiFailure } from '../../lib/api/errors';
  import type { DerivativeNode, Project } from '../../lib/api/types';
  import { derivativesPageHref, inspectionPageHref, projectPageHref } from '../../lib/links';

  interface Props {
    projectId: string;
    locale: Locale;
    m: Messages;
  }

  type DerivativePolicy = Project['derivativePolicy'];
  type TreeRelation = 'current' | 'ancestor' | 'descendant' | 'related';

  interface TreeItem {
    id: string;
    slug: string;
    title: string;
    ownerHandle: string;
    parentId: string | null;
    depth: number;
    updatedAt: string;
    derivativePolicy?: DerivativePolicy;
    current: boolean;
    relation: TreeRelation;
    children: TreeItem[];
  }

  let { projectId, locale, m }: Props = $props();
  let project = $state<Project>();
  let derivativeNodes = $state<DerivativeNode[]>([]);
  let state = $state<'loading' | 'ready' | 'error'>('loading');
  let failure = $state<ApiFailure>('unknown');
  let treeFailure = $state<ApiFailure>();
  let forest = $derived.by(() => buildTreeForest(project, derivativeNodes));

  // 孫以下（depth >= 2）の遅延展開。直接の子（depth 1）は初期表示する。
  let expanded = $state<Set<string>>(new Set());

  // 派生作成のフォーム状態。
  let showDeriveForm = $state(false);
  let deriveTitle = $state('');
  let deriveDescription = $state('');
  let deriveVisibility = $state<'public' | 'private'>('private');
  let deriving = $state(false);
  let deriveResult = $state<string>();
  let deriveFailure = $state<ApiFailure>();

  const devHeaders = import.meta.env.DEV
    ? { Authorization: 'Bearer mock-user-token' }
    : undefined;

  function toggleExpand(nodeId: string) {
    const next = new Set(expanded);
    if (next.has(nodeId)) next.delete(nodeId);
    else next.add(nodeId);
    expanded = next;
  }

  /// 現在のプロジェクトと直接の子（depth 1）は初期表示する。
  function isAutoExpanded(node: TreeItem): boolean {
    return node.current || node.depth <= 1;
  }

  /// ノードの子を表示するか。
  function isNodeOpen(node: TreeItem): boolean {
    return isAutoExpanded(node) || expanded.has(node.id);
  }

  async function submitDerive() {
    deriveResult = undefined;
    deriveFailure = undefined;
    if (!deriveTitle.trim()) {
      deriveFailure = 'validation_failed';
      return;
    }
    deriving = true;
    try {
      const result = await apiClient.POST('/projects/{projectId}/derive', {
        params: { path: { projectId } },
        headers: devHeaders,
        body: {
          title: deriveTitle.trim(),
          description: deriveDescription.trim() || undefined,
          visibility: deriveVisibility,
        },
      });
      if (result.error) {
        deriveFailure = mapApiFailure(result.error, result.response.status);
        return;
      }
      deriveResult = result.data.jobId;
      deriveTitle = '';
      deriveDescription = '';
      showDeriveForm = false;
      await load();
    } catch {
      deriveFailure = 'upstream-unavailable';
    } finally {
      deriving = false;
    }
  }

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale === 'ja' ? 'ja-JP' : 'en-US', {
      dateStyle: 'medium',
      timeZone: 'UTC',
    }).format(date);
  }

  function buildTreeForest(currentProject: Project | undefined, nodes: DerivativeNode[]): TreeItem[] {
    if (!currentProject) return [];

    const currentRecord = nodes.find((node) => node.id === currentProject.id);
    const items = new Map<string, TreeItem>();
    for (const node of nodes) {
      if (node.id === currentProject.id) continue;
      items.set(node.id, {
        id: node.id,
        slug: node.slug,
        title: node.title,
        ownerHandle: node.ownerHandle,
        parentId: node.parentId ?? null,
        depth: node.depth,
        updatedAt: node.updatedAt,
        derivativePolicy: node.derivativePolicy,
        current: false,
        relation: 'related',
        children: [],
      });
    }

    const currentItem: TreeItem = {
      id: currentProject.id,
      slug: currentProject.slug,
      title: currentProject.title,
      ownerHandle: currentProject.ownerHandle,
      parentId: currentRecord?.parentId ?? null,
      depth: 0,
      updatedAt: currentProject.updatedAt,
      derivativePolicy: currentProject.derivativePolicy,
      current: true,
      relation: 'current',
      children: [],
    };
    items.set(currentProject.id, currentItem);

    const linked = new Set<string>();
    for (const item of items.values()) {
      if (!item.parentId || item.parentId === item.id) continue;
      const parent = items.get(item.parentId);
      if (!parent) continue;

      // Ignore malformed cyclic relationships so the displayed tree stays finite.
      let cursor: TreeItem | undefined = parent;
      const visited = new Set<string>();
      let createsCycle = false;
      while (cursor && !visited.has(cursor.id)) {
        if (cursor.id === item.id) {
          createsCycle = true;
          break;
        }
        visited.add(cursor.id);
        cursor = cursor.parentId ? items.get(cursor.parentId) : undefined;
      }
      if (!createsCycle) {
        parent.children.push(item);
        linked.add(item.id);
      }
    }

    const ancestorIds = new Set<string>();
    let ancestorId = currentItem.parentId;
    while (ancestorId && !ancestorIds.has(ancestorId)) {
      ancestorIds.add(ancestorId);
      ancestorId = items.get(ancestorId)?.parentId ?? null;
    }
    const descendantIds = new Set<string>();
    const collectDescendants = (item: TreeItem) => {
      for (const child of item.children) {
        if (descendantIds.has(child.id)) continue;
        descendantIds.add(child.id);
        collectDescendants(child);
      }
    };
    collectDescendants(currentItem);

    for (const item of items.values()) {
      if (item.current) item.relation = 'current';
      else if (ancestorIds.has(item.id)) item.relation = 'ancestor';
      else if (descendantIds.has(item.id)) item.relation = 'descendant';
      else item.relation = 'related';
    }

    const roots = [...items.values()].filter((item) => !linked.has(item.id));
    if (roots.length === 0) return [currentItem];

    const containsCurrent = (item: TreeItem, visited = new Set<string>()): boolean => {
      if (item.id === currentProject.id) return true;
      if (visited.has(item.id)) return false;
      visited.add(item.id);
      return item.children.some((child) => containsCurrent(child, visited));
    };
    const currentBranch = roots.find((root) => containsCurrent(root));
    if (!currentBranch) return [currentItem, ...roots.filter((root) => root.id !== currentItem.id)];
    return [currentBranch, ...roots.filter((root) => root.id !== currentBranch.id)];
  }

  async function load() {
    if (!projectId) {
      failure = 'not-found';
      state = 'error';
      return;
    }

    try {
      const [projectResult, derivativesResult] = await Promise.allSettled([
        apiClient.GET('/projects/{projectId}', { params: { path: { projectId } } }),
        apiClient.GET('/projects/{projectId}/derivatives', { params: { path: { projectId } } }),
      ]);

      if (projectResult.status === 'rejected') {
        failure = 'upstream-unavailable';
        state = 'error';
        return;
      }
      if (projectResult.value.error) {
        failure = mapApiFailure(projectResult.value.error, projectResult.value.response.status);
        state = 'error';
        return;
      }
      project = projectResult.value.data;

      if (derivativesResult.status === 'rejected') {
        treeFailure = 'upstream-unavailable';
      } else if (derivativesResult.value.error) {
        treeFailure = mapApiFailure(
          derivativesResult.value.error,
          derivativesResult.value.response.status,
        );
      } else {
        derivativeNodes = derivativesResult.value.data;
        // 直接の子（depth 1）を初期表示する。孫以下は遅延させる。
        const direct = derivativesResult.value.data.filter((n) => n.depth === 1);
        expanded = new Set(direct.map((n) => n.id));
      }
      state = 'ready';
    } catch {
      failure = 'upstream-unavailable';
      state = 'error';
    }
  }

  onMount(() => {
    void load();
  });
</script>

{#snippet renderTreeNode(node: TreeItem)}
  <li class:derivative-current={node.current}>
    <article class="derivative-node-card">
      <div class="derivative-node-heading">
        <a href={projectPageHref(node.id)} aria-current={node.current ? 'page' : undefined}>
          {node.title}
        </a>
        <span class="badge">{m.derivatives.relations[node.relation]}</span>
        {#if node.children.length > 0 && !isAutoExpanded(node)}
          <button
            type="button"
            class="derivative-expand"
            aria-expanded={isNodeOpen(node)}
            onclick={() => toggleExpand(node.id)}
          >
            {isNodeOpen(node) ? m.derivatives.collapse : `${m.derivatives.expand}（${m.derivatives.depthLazy}）`}
          </button>
        {/if}
      </div>
      <div class="derivative-node-meta">
        <span>{m.derivatives.owner}: {node.ownerHandle}</span>
        <span>{m.derivatives.updated}: <time datetime={node.updatedAt}>{formatDate(node.updatedAt)}</time></span>
        {#if node.derivativePolicy}
          <span class="badge">{m.derivatives.policies[node.derivativePolicy]}</span>
        {:else}
          <span class="badge">{m.derivatives.policyUnknown}</span>
        {/if}
      </div>
      <p class="derivative-node-id">{node.slug}</p>
    </article>
    {#if node.children.length > 0 && isNodeOpen(node)}
      <ul>
        {#each node.children as child (child.id)}
          {@render renderTreeNode(child)}
        {/each}
      </ul>
    {/if}
  </li>
{/snippet}

<section class="derivatives-page section-stack" aria-labelledby="derivatives-heading">
  {#if project}
    <p class="breadcrumbs">
      <a href={projectPageHref(projectId)}>{m.derivatives.backToProject}</a>
    </p>
    <nav class="project-tabs" aria-label={m.project.sectionsLabel}>
      <a href={projectPageHref(projectId)}>{m.project.overviewTab}</a>
      <a href={derivativesPageHref(projectId)} aria-current="page">{m.project.derivativesTab}</a>
      <a href={inspectionPageHref(projectId)}>{m.project.inspectionTab}</a>
    </nav>
  {:else}
    <p class="breadcrumbs"><a href="/">{m.project.back}</a></p>
  {/if}

  <header class="page-heading">
    <p class="publish-eyebrow">{m.derivatives.eyebrow}</p>
    <h1 id="derivatives-heading">
      {#if project}{project.title} — {m.derivatives.title}{:else}{m.derivatives.title}{/if}
    </h1>
    <p>{m.derivatives.lead}</p>
  </header>

  {#if state === 'loading'}
    <p role="status">{m.api.loading}</p>
  {:else if state === 'error'}
    <article class="panel">
      <h2>{m.derivatives.title}</h2>
      <p role="alert">{m.api.errors[failure]}</p>
    </article>
  {:else if project}
    <section class="panel derivative-tree-panel" aria-labelledby="derivative-tree-heading">
      <div class="derivative-tree-intro">
        <div>
          <h2 id="derivative-tree-heading">{m.derivatives.treeTitle}</h2>
          <p>{m.derivatives.treeHelp}</p>
          {#if project?.derivativePolicy !== 'disallowed'}
            <p class="derivative-create-help">{m.derivatives.createHelp}</p>
          {/if}
        </div>
        <span class="badge">
          {m.derivatives.nodeCount(derivativeNodes.filter((node) => node.id !== project?.id).length)}
        </span>
      </div>

      {#if project?.derivativePolicy !== 'disallowed'}
        {#if !showDeriveForm}
          <button type="button" class="derivative-create-button" onclick={() => (showDeriveForm = true)}>
            {m.derivatives.createTitle}
          </button>
        {:else}
          <form class="derivative-create-form" onsubmit={(e) => { e.preventDefault(); void submitDerive(); }}>
            <h3>{m.derivatives.formTitle}</h3>
            <label>
              <span>{m.derivatives.titleLabel}</span>
              <input type="text" bind:value={deriveTitle} placeholder={m.derivatives.titlePlaceholder} required />
            </label>
            <label>
              <span>{m.derivatives.descriptionLabel}</span>
              <textarea bind:value={deriveDescription} placeholder={m.derivatives.descriptionPlaceholder}></textarea>
            </label>
            <label>
              <span>{m.derivatives.visibilityLabel}</span>
              <select bind:value={deriveVisibility}>
                <option value="private">{m.derivatives.visibilityPrivate}</option>
                <option value="public">{m.derivatives.visibilityPublic}</option>
              </select>
            </label>
            {#if deriveFailure}
              <p role="alert">{m.api.errors[deriveFailure] ?? m.derivatives.createError}</p>
            {/if}
            <div class="derivative-create-actions">
              <button type="submit" disabled={deriving}>
                {deriving ? m.derivatives.creating : m.derivatives.submit}
              </button>
              <button type="button" onclick={() => (showDeriveForm = false)} disabled={deriving}>
                {m.project.cancel}
              </button>
            </div>
          </form>
        {/if}
        {#if deriveResult}
          <p class="derivative-create-success" role="status">{m.derivatives.created(deriveResult)}</p>
        {/if}
      {/if}
      {#if treeFailure}
        <p role="alert">{m.api.errors[treeFailure]}</p>
      {:else if derivativeNodes.length === 0}
        <p class="derivative-empty">{m.derivatives.empty}</p>
        <ul class="derivative-tree" aria-label={m.derivatives.treeTitle}>
          {@render renderTreeNode(forest[0])}
        </ul>
      {:else}
        <ul class="derivative-tree" aria-label={m.derivatives.treeTitle}>
          {#each forest as root (root.id)}
            {@render renderTreeNode(root)}
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</section>
