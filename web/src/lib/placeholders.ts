import type { Locale } from './i18n';

// UI skeleton view models only. These are not API contracts or server models.
export type LocalizedText = Record<Locale, string>;
export type ProjectStatus = 'draft' | 'active';

export interface PlaceholderProject {
  id: string;
  title: LocalizedText;
  summary: LocalizedText;
  status: ProjectStatus;
  updatedAt: string;
  chapterIds: string[];
}

export interface PlaceholderChapter {
  id: string;
  projectId: string;
  title: LocalizedText;
  paragraphs: LocalizedText[];
}

export const placeholderProjects: PlaceholderProject[] = [
  {
    id: 'demo-project',
    title: {
      ja: 'サンプルプロジェクト',
      en: 'Sample project',
    },
    summary: {
      ja: 'プロジェクトページのレイアウトを確認するためのプレースホルダーです。',
      en: 'A placeholder for checking the project page layout.',
    },
    status: 'draft',
    updatedAt: '2026-01-15',
    chapterIds: ['demo-chapter-1', 'demo-chapter-2'],
  },
  {
    id: 'archive-demo',
    title: {
      ja: '空状態サンプル',
      en: 'Empty-state sample',
    },
    summary: {
      ja: 'チャプターがない場合の表示を確認するためのプレースホルダーです。',
      en: 'A placeholder for checking the empty chapter state.',
    },
    status: 'active',
    updatedAt: '2026-01-10',
    chapterIds: [],
  },
];

export const placeholderChapters: PlaceholderChapter[] = [
  {
    id: 'demo-chapter-1',
    projectId: 'demo-project',
    title: {
      ja: '第1章: 骨組み',
      en: 'Chapter 1: Skeleton',
    },
    paragraphs: [
      {
        ja: 'これは読者ビューの骨組みを確認するためのプレースホルダー本文です。',
        en: 'This placeholder body text is here to exercise the reader view skeleton.',
      },
      {
        ja: '実際のデータ取得や API 接続はまだ行っていません。',
        en: 'No real data fetching or API connection is implemented yet.',
      },
      {
        ja: '言語とテーマを切り替えても、読み幅と余白の基本レイアウトを確認できます。',
        en: 'Switching language and theme keeps the basic reading width and spacing visible.',
      },
    ],
  },
  {
    id: 'demo-chapter-2',
    projectId: 'demo-project',
    title: {
      ja: '第2章: 続き',
      en: 'Chapter 2: Continuation',
    },
    paragraphs: [
      {
        ja: '前後のチャプターへの導線を確認するためのプレースホルダーです。',
        en: 'This placeholder checks navigation to previous and next chapters.',
      },
      {
        ja: '本文、見出し、ナビゲーションの配置は今後の仕様確定後に調整します。',
        en: 'Body, heading, and navigation placement will be adjusted after the specifications are settled.',
      },
    ],
  },
];

export function getProject(id: string): PlaceholderProject | undefined {
  return placeholderProjects.find((project) => project.id === id);
}

export function getChapter(id: string): PlaceholderChapter | undefined {
  return placeholderChapters.find((chapter) => chapter.id === id);
}

export function getProjectChapters(projectId: string): PlaceholderChapter[] {
  const project = getProject(projectId);
  if (!project) return [];

  return project.chapterIds
    .map((id) => getChapter(id))
    .filter((chapter): chapter is PlaceholderChapter => chapter !== undefined);
}
