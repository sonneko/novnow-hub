import type { Project } from './api/types';

export interface ProjectIndexFilters {
  license?: Project['license'];
  tag?: string;
}

/**
 * A tiny in-memory index of pages already returned by the API. It deduplicates
 * cursor pages and makes filter-only browsing possible without an extra route.
 */
export class ProjectIndex {
  private readonly projects = new Map<string, Project>();

  clear(): void {
    this.projects.clear();
  }

  add(items: Project[]): void {
    for (const project of items) this.projects.set(project.id, project);
  }

  search(filters: ProjectIndexFilters = {}): Project[] {
    const tag = filters.tag?.trim().toLocaleLowerCase();
    return [...this.projects.values()].filter((project) => {
      if (filters.license && project.license !== filters.license) return false;
      if (tag && !project.tags?.some((item) => item.toLocaleLowerCase().includes(tag))) return false;
      return true;
    });
  }
}
