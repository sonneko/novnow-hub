# novnow-hub

novnow 形式の作品を公開・共有する Web プラットフォーム。非エンジニアが novnow 形式のファイルをサッと Web 公開でき、二次創作者が URL ひとつでアプリ内に自動セットアップしたり、URL だけでセットアップ済みの novnow をブラウザで開いたりできる。GitHub のように変更提案（PR 相当）+ 課題（Issue 相当）+ 自動マージ + チャットが使え、novnow 特化の機能も提供する。

- ソフトウェアとして OSS 公開（MIT License）を前提とし、サーバー費用は最小限に抑える設計
- フロントエンド: Astro + Svelte + TypeScript（Tauri でマルチプラットフォーム化）
- Phase 1 は GitHub を土台（公開プロジェクト＝GitHub リポジトリ）。ただし GitHub 依存はポート／アダプターの背後に隔離され、API 契約とフロントエンドは GitHub を知らない（`REF:docs/specifications/api.md` と `REF:docs/handoff.md` 参照）
- バックエンドは別途開発（Rust + Axum + SQLite 等を候補）。フロントエンドは API 契約（OpenAPI 3.1）のみに依存する

## 現在のステータス

M2（フロントエンド基盤とモック接続）は CI 拡張を残してほぼ完了。M3・M4 は進行中で、M3 の読者ビュー／公開ページキャッシュと M4 のコラボレーション UI・チャットの SQLite 参照実装を追加済み。未完了項目の詳細は `REF:docs/plans.md` を参照。

## ドキュメント

- ユーザーから見える振る舞いの仕様: `docs/specifications/overview.md`
- API 契約の仕様: `docs/specifications/api.md`（機械可読な契約は `api/openapi.yaml`）
- 実装設計: `docs/designs/overview.md` / `docs/designs/api.md`
- 開発計画と進捗: `docs/plans.md`
- 開発者向け固定情報（規則・環境）: `docs/developers.md`
- 変更履歴・意思決定アーカイブ: `docs/history.md`

## 開発の進め方

このリポジトリは仕様駆動開発のテンプレートを土台にしている。作業前に `docs/developers.md` のドキュメント管理規則と `docs/handoff.md` を読むこと。`scripts/verify.sh` と `python3 docs/script/verify-docs.py` がコミット前の検証として使える。

## 試し方

この段階ではバックエンドは未導入で、モック API（OpenAPI 契約から応答を生成する Prism）と組み合わせてフロントエンドのみ試せる。**公開済みのデプロイ URL はまだ存在しない**ため、以下の手順はローカル環境のみで有効である。

### 前提条件

- Node.js 22.12.0 以上（`web/package.json` の `engines.node` と一致）
- 依存インストールに npm とネットワークアクセス

### インストール

```sh
cd web
npm ci
```

### 開発サーバーの起動

2 つのターミナルで起動する。

```sh
# terminal 1: OpenAPI 契約から応答を生成するモック API（:4010）
npm run mock

# terminal 2: Web UI（:4321）。/api/v1 をモック API へプロキシする
npm run dev
```

起動後、ブラウザで <http://localhost:4321/> を開く。

### 試せることの一覧

モック API は契約の example 応答を返すため、以下は今日実際に試せる範囲である（バックエンド導入後の本番挙動とは異なる）。

- **作品一覧の閲覧**: `/`（モックの `GET /projects` の example データを表示）
- **作品ページ**: `/projects/?projectId=<id>`（任意の projectId で対応）
- **読者ビュー**: `/reading/?projectId=<id>`（本文表示と ETag キャッシュ）
- **公開フロー（ウィザード UI）**: `/publish`（非エンジニア向け 4 ステップの公開ウィザード画面）
- **変更提案・課題**: `/collaboration/proposals/?projectId=<id>`、`/collaboration/issues/?projectId=<id>`
- **チャット**: `/collaboration/chat/?projectId=<id>`（SQLite バックのチャット API を使う場合は別途 `npm run chat:dev` を起動）
- **派生作品一覧**: `/derivatives`
- **セットアップ URL 解決**: `/setup/resolve?u=<URL>`

未実装（この段階では試せない）: ログイン（GitHub OAuth）・バックエンド連携の公開実体・自動マージ・通知センター・検索。

## ライセンス

[MIT License](./LICENSE)
