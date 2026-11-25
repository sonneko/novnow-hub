export type ShellPage =
  | { kind: 'projects' }
  | { kind: 'project'; projectId?: string }
  | { kind: 'reader'; projectId?: string }
  | { kind: 'derivatives'; projectId?: string }
  | { kind: 'inspection'; projectId?: string }
  | { kind: 'setupResolve' }
  | { kind: 'publish' }
  | { kind: 'proposals'; projectId?: string }
  | { kind: 'proposal'; projectId?: string; number?: number }
  | { kind: 'issues'; projectId?: string }
  | { kind: 'issue'; projectId?: string; number?: number }
  | { kind: 'chat'; projectId?: string }
  | { kind: 'notifications' };
