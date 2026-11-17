import type {
  DerivativeNode,
  FileContent,
  ReadingDocument,
  Project,
  ProjectPage,
  SetupInfo,
  Tree,
} from '../lib/api/types';

export const projectFixture = {
  id: 'project-1',
  slug: 'sample-story',
  title: 'サンプル小説',
  description: 'API クライアント経由のサンプル作品です。',
  ownerHandle: 'sora',
  license: 'cc-by',
  visibility: 'public',
  tags: ['fantasy', 'short'],
  derivativePolicy: 'allowed',
  derivativeCount: 1,
  defaultBranchName: 'main',
  etag: 'W/"project-1"',
  createdAt: '2026-01-10T00:00:00Z',
  updatedAt: '2026-01-15T12:00:00Z',
  publishedUrl: 'https://hub.example.test/sora/sample-story',
} satisfies Project;

export const projectPageFixture = {
  items: [projectFixture],
  nextCursor: null,
} satisfies ProjectPage;

export const derivativeNodesFixture = [
  {
    id: 'source-1',
    slug: 'original-story',
    title: '原作・星を渡る',
    ownerHandle: 'hoshi',
    depth: 1,
    parentId: null,
    derivativePolicy: 'allowed',
    createdAt: '2025-11-01T00:00:00Z',
    updatedAt: '2025-12-18T10:00:00Z',
  },
  {
    id: projectFixture.id,
    slug: projectFixture.slug,
    title: projectFixture.title,
    ownerHandle: projectFixture.ownerHandle,
    depth: 2,
    parentId: 'source-1',
    derivativePolicy: projectFixture.derivativePolicy,
    createdAt: projectFixture.createdAt,
    updatedAt: projectFixture.updatedAt,
  },
  {
    id: 'sibling-1',
    slug: 'original-story-another-view',
    title: 'もうひとつの星',
    ownerHandle: 'tsuki',
    depth: 2,
    parentId: 'source-1',
    derivativePolicy: 'disallowed',
    createdAt: '2026-01-12T00:00:00Z',
    updatedAt: '2026-02-03T11:00:00Z',
  },
  {
    id: 'derivative-1',
    slug: 'sample-story-afterglow',
    title: 'サンプル小説・その後',
    ownerHandle: 'mizu',
    depth: 3,
    parentId: projectFixture.id,
    derivativePolicy: 'allowed-with-conditions',
    createdAt: '2026-02-01T00:00:00Z',
    updatedAt: '2026-02-15T12:00:00Z',
  },
  {
    id: 'derivative-1-1',
    slug: 'sample-story-afterglow-side-story',
    title: '灯りの向こう側',
    ownerHandle: 'nagi',
    depth: 4,
    parentId: 'derivative-1',
    derivativePolicy: 'allowed',
    createdAt: '2026-02-20T00:00:00Z',
    updatedAt: '2026-03-02T08:30:00Z',
  },
] satisfies DerivativeNode[];

export const treeFixture = {
  entries: [
    { path: 'novnow/01-opening.md', kind: 'file', size: 1280 },
    { path: 'novnow/notes', kind: 'dir', size: 0 },
  ],
  truncated: false,
} satisfies Tree;

export const readingDocumentFixture = {
  projectId: projectFixture.id,
  title: 'サンプル小説',
  blocks: [
    { kind: 'heading', text: '第一章' },
    { kind: 'paragraph', text: 'API から取得した本文です。' },
    { kind: 'dialogue', text: 'ここにいるよ。', speaker: 'ミナ' },
    { kind: 'scene_break', text: '＊ ＊ ＊' },
  ],
  etag: 'W/"reading-1"',
} satisfies ReadingDocument;

export const fileContentFixture = {
  path: 'novnow/01-opening.md',
  content: '# 第一章\n\nこれは作品ファイルのプレビューです。',
  encoding: 'utf8',
  etag: 'W/"file-1"',
  size: 64,
} satisfies FileContent;

export const setupInfoFixture = {
  projectId: projectFixture.id,
  slug: projectFixture.slug,
  snapshotVersion: 'v1',
  acquisition: {
    kind: 'codeload',
    url: 'https://hub.example.test/setup/sample-story.zip',
  },
  novnow: { minVersion: '1.2.0' },
} satisfies SetupInfo;
