export type Locale = 'ja' | 'en';

const ja = {
  app: {
    name: 'novnow-hub',
    skipToContent: '本文へ移動',
  },
  nav: {
    primary: 'メインナビゲーション',
    projects: 'プロジェクト',
    reader: '読者ビュー',
  },
  controls: {
    languageName: 'English',
    switchLanguage: 'Switch to English',
    dark: 'ダーク',
    light: 'ライト',
    switchToDark: 'ダークテーマに切り替え',
    switchToLight: 'ライトテーマに切り替え',
  },
  notice: {
    placeholder: 'プレースホルダーデータ表示中',
    noApi: 'API 未接続',
  },
  projects: {
    title: 'プロジェクト',
    lead: 'プレースホルダーデータによるプロジェクト一覧の骨格です。',
    updated: '更新日',
    empty: '表示できるプロジェクトはありません。',
    status: {
      draft: '下書き',
      active: '公開中',
    },
  },
  project: {
    title: 'プロジェクトページ',
    overview: '概要',
    chapters: 'チャプター',
    activity: 'アクティビティ',
    activityPlaceholder: 'アクティビティはプレースホルダーです。',
    back: 'プロジェクト一覧へ',
    read: '読む',
    emptyChapters: 'チャプターはまだありません。',
    notFound: 'プロジェクトが見つかりません。',
  },
  reader: {
    title: '読者ビュー',
    navigation: 'チャプター移動',
    backToProject: 'プロジェクトへ',
    previous: '前へ',
    next: '次へ',
    notFound: 'チャプターが見つかりません。',
    placeholderNote: '本文はプレースホルダーです。API 接続は行っていません。',
  },
};

export type Messages = typeof ja;

const en: Messages = {
  app: {
    name: 'novnow-hub',
    skipToContent: 'Skip to content',
  },
  nav: {
    primary: 'Main navigation',
    projects: 'Projects',
    reader: 'Reader',
  },
  controls: {
    languageName: '日本語',
    switchLanguage: '日本語に切り替え',
    dark: 'Dark',
    light: 'Light',
    switchToDark: 'Switch to dark theme',
    switchToLight: 'Switch to light theme',
  },
  notice: {
    placeholder: 'Showing placeholder data',
    noApi: 'No API connection',
  },
  projects: {
    title: 'Projects',
    lead: 'A project list skeleton driven by placeholder data.',
    updated: 'Updated',
    empty: 'No projects to display.',
    status: {
      draft: 'Draft',
      active: 'Active',
    },
  },
  project: {
    title: 'Project page',
    overview: 'Overview',
    chapters: 'Chapters',
    activity: 'Activity',
    activityPlaceholder: 'Activity is a placeholder.',
    back: 'Back to projects',
    read: 'Read',
    emptyChapters: 'No chapters yet.',
    notFound: 'Project not found.',
  },
  reader: {
    title: 'Reader',
    navigation: 'Chapter navigation',
    backToProject: 'Back to project',
    previous: 'Previous',
    next: 'Next',
    notFound: 'Chapter not found.',
    placeholderNote: 'This body text is a placeholder. No API connection is used.',
  },
};

export const messages: Record<Locale, Messages> = { ja, en };
