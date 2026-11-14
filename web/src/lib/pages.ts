export type ShellPage =
  | { kind: 'projects' }
  | { kind: 'project'; projectId: string }
  | { kind: 'reader'; chapterId: string };
