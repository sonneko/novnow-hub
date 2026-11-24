export function projectPageHref(projectId: string): string {
  return `/projects/?projectId=${encodeURIComponent(projectId)}`;
}

export function readerPageHref(projectId: string): string {
  return `/reading/?projectId=${encodeURIComponent(projectId)}`;
}

export function derivativesPageHref(projectId: string): string {
  return `/derivatives/?projectId=${encodeURIComponent(projectId)}`;
}

export function setupResolveHref(setupUrl: string): string {
  return `/setup/resolve/?u=${encodeURIComponent(setupUrl)}`;
}

export function proposalListHref(projectId: string): string {
  return `/collaboration/proposals/?projectId=${encodeURIComponent(projectId)}`;
}

export function proposalDetailHref(projectId: string, number: number): string {
  return `/collaboration/proposals/detail/?projectId=${encodeURIComponent(projectId)}&number=${number}`;
}

export function issueListHref(projectId: string): string {
  return `/collaboration/issues/?projectId=${encodeURIComponent(projectId)}`;
}

export function issueDetailHref(projectId: string, number: number): string {
  return `/collaboration/issues/detail/?projectId=${encodeURIComponent(projectId)}&number=${number}`;
}

export function chatPageHref(projectId: string): string {
  return `/collaboration/chat/?projectId=${encodeURIComponent(projectId)}`;
}

export function notificationsPageHref(): string {
  return '/notifications/';
}
