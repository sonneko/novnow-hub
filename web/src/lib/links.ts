export function projectPageHref(projectId: string): string {
  return `/projects/?projectId=${encodeURIComponent(projectId)}`;
}

export function readerPageHref(projectId: string): string {
  return `/read/?projectId=${encodeURIComponent(projectId)}`;
}
