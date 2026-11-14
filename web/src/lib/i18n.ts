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
    mockData: 'OpenAPI 契約に基づくモック API のデータを表示中です。',
  },
  api: {
    loading: '読み込み中です…',
    errors: {
      'auth-required': 'この情報を見るにはサインインが必要です。',
      'not-found': '要求した情報が見つかりません。',
      conflict: 'データが更新されています。ページを再読み込みしてください。',
      'rate-limited': 'アクセスが集中しています。しばらくしてから再試行してください。',
      'upstream-unavailable': 'API を一時的に利用できません。',
      'verification-failed': 'データの検証に失敗しました。',
      unknown: 'データを読み込めませんでした。',
    },
  },
  projects: {
    title: 'プロジェクト',
    lead: '公開プロジェクトを一覧できます。',
    updated: '更新日',
    owner: '作者',
    empty: '表示できるプロジェクトはありません。',
    noDescription: '説明はありません。',
    visibility: {
      public: '公開',
      private: '非公開',
    },
  },
  project: {
    title: 'プロジェクト',
    overview: '概要',
    contents: '内容',
    owner: '作者',
    license: 'ライセンス',
    tags: 'タグ',
    updated: '更新日',
    openReader: '読者ビューで読む',
    back: 'プロジェクト一覧へ',
    emptyContents: '表示できる内容はありません。',
    truncatedContents: '内容の一部のみ表示しています。',
    file: 'ファイル',
    directory: 'フォルダー',
    notFound: 'プロジェクトが見つかりません。',
  },
  reader: {
    title: '読者ビュー',
    backToProject: 'プロジェクトへ戻る',
    empty: '本文はまだありません。',
    sceneBreak: '場面の区切り',
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
    switchLanguage: 'Switch to Japanese',
    dark: 'Dark',
    light: 'Light',
    switchToDark: 'Switch to dark theme',
    switchToLight: 'Switch to light theme',
  },
  notice: {
    mockData: 'Showing data from the OpenAPI-based mock API.',
  },
  api: {
    loading: 'Loading…',
    errors: {
      'auth-required': 'Sign in is required to view this information.',
      'not-found': 'The requested information could not be found.',
      conflict: 'The data has changed. Reload the page and try again.',
      'rate-limited': 'Too many requests. Please try again shortly.',
      'upstream-unavailable': 'The API is temporarily unavailable.',
      'verification-failed': 'The data could not be verified.',
      unknown: 'The data could not be loaded.',
    },
  },
  projects: {
    title: 'Projects',
    lead: 'Browse public projects.',
    updated: 'Updated',
    owner: 'Author',
    empty: 'No projects to display.',
    noDescription: 'No description provided.',
    visibility: {
      public: 'Public',
      private: 'Private',
    },
  },
  project: {
    title: 'Project',
    overview: 'Overview',
    contents: 'Contents',
    owner: 'Author',
    license: 'License',
    tags: 'Tags',
    updated: 'Updated',
    openReader: 'Read in the reader',
    back: 'Back to projects',
    emptyContents: 'No content to display.',
    truncatedContents: 'Only part of the content is shown.',
    file: 'File',
    directory: 'Folder',
    notFound: 'Project not found.',
  },
  reader: {
    title: 'Reader',
    backToProject: 'Back to project',
    empty: 'There is no reading content yet.',
    sceneBreak: 'Scene break',
  },
};

export const messages: Record<Locale, Messages> = { ja, en };
