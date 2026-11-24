import type {
  AutomergeState,
  CommentPageData,
  Issue,
  IssuePageData,
  Notification,
  NotificationPageData,
  Proposal,
  ProposalDiff,
  ProposalPageData,
} from '../lib/api/types';

export const proposalFixture = {
  number: 24,
  title: '場面描写を追加',
  body: '第二章の描写を補いました。',
  state: 'open',
  authorHandle: 'sora',
  baseRef: 'main',
  headRef: 'scene-revision',
  diffStats: { additions: 8, deletions: 2, changedFiles: 1 },
  checks: [{ name: 'novnow 形式チェック', state: 'success' }],
  approvalCount: 0,
  requiredApprovals: 1,
  createdAt: '2026-10-12T10:00:00Z',
  updatedAt: '2026-10-13T12:30:00Z',
} satisfies Proposal;

export const proposalPageFixture = {
  items: [proposalFixture],
  nextCursor: null,
} satisfies ProposalPageData;

export const proposalDiffFixture = {
  number: proposalFixture.number,
  files: [
    {
      path: 'novnow/02-chapter.md',
      status: 'modified',
      additions: 8,
      deletions: 2,
      hunks: ['@@ -12,2 +12,8 @@\n 古い一文\n+新しい一文\n+続きの描写'],
    },
  ],
  novnowDiagnosticsDelta: { introduced: 0, resolved: 1 },
} satisfies ProposalDiff;

export const automergeStateFixture = {
  state: 'reserved',
  requiredApprovals: 1,
  requiredChecks: ['novnow 形式チェック'],
  requireNoConflicts: true,
  etag: 'automerge-etag-1',
} satisfies AutomergeState;

export const notificationFixture = {
  id: 'notif-1',
  kind: 'proposal_merged',
  projectId: 'project-1',
  projectSlug: 'moonlit-garden',
  title: '変更提案 #24 が取り込まれました',
  body: '場面描写を追加 の提案がマージされました。',
  read: false,
  createdAt: '2026-10-13T12:30:00Z',
} satisfies Notification;

export const notificationUnreadFixture = {
  ...notificationFixture,
  id: 'notif-2',
  kind: 'issue_comment',
  title: '課題 #7 にコメントがありました',
  body: 'mizu がコメントを追加しました。',
  read: false,
  createdAt: '2026-10-13T13:00:00Z',
} satisfies Notification;

export const notificationPageFixture = {
  items: [notificationFixture, notificationUnreadFixture],
  nextCursor: null,
} satisfies NotificationPageData;

export const issueFixture = {
  number: 7,
  title: '目次の順序を確認したい',
  body: '章の順序が表示と一致しているか確認してください。',
  state: 'open',
  labels: ['content', 'question'],
  authorHandle: 'mizu',
  assigneeHandle: 'sora',
  linkedProposalNumber: proposalFixture.number,
  createdAt: '2026-10-10T08:00:00Z',
  updatedAt: '2026-10-11T09:00:00Z',
} satisfies Issue;

export const issuePageFixture = {
  items: [issueFixture],
  nextCursor: null,
} satisfies IssuePageData;

export const commentPageFixture = {
  items: [
    {
      id: 'comment-1',
      authorHandle: 'mizu',
      body: '第二章の描写は前後の流れと合っていますか。',
      path: 'novnow/02-chapter.md',
      line: 14,
      createdAt: '2026-10-12T11:00:00Z',
    },
    {
      id: 'comment-2',
      authorHandle: 'sora',
      body: '合っていると思います。\n次の場面にもつながります。',
      createdAt: '2026-10-12T12:30:00Z',
    },
  ],
  nextCursor: null,
} satisfies CommentPageData;
