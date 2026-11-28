# novnow-hub

novnow 形式の作品を公開・共有する Web プラットフォーム。非エンジニアが novnow 形式のファイルをサッと Web 公開でき、二次創作者が URL ひとつでアプリ内に自動セットアップしたり、URL だけでセットアップ済みの novnow をブラウザで開いたりできる。GitHub のように変更提案（PR 相当）+ 課題（Issue 相当）+ 自動マージ + チャットが使え、novnow 特化の機能も提供する。

- ソフトウェアとして OSS 公開（MIT License）を前提とし、サーバー費用は最小限に抑える設計
- フロントエンド: Astro + Svelte + TypeScript（Tauri でマルチプラットフォーム化）
- Phase 1 は GitHub を土台（公開プロジェクト＝GitHub リポジトリ）。ただし GitHub 依存はポート／アダプターの背後に隔離され、API 契約とフロントエンドは GitHub を知らない（`REF:docs/specifications/api.md` と `REF:docs/handoff.md` 参照）
- バックエンドは `server/` crate（Rust + Axum、ネイティブと Cloudflare Workers 用 wasm の両ターゲット）。フロントエンドは API 契約（OpenAPI 3.1）のみに依存する

## 現在のステータス

M1〜M6 は完了。M7（運用・品質・OSS 公開準備）は §2（本番デプロイ・外形監視・バックアップ・鍵ローテーション）・§3（CI 拡張）・§5（ドキュメント整備）・§6（測定結果の記録）が完了し、§4 の残項目（CSP・依存ライセンスの棚卸し）と §5.3 の初回リリース（タグ・GitHub Release）、§6 の実測が後続タスク。API 契約（`api/openapi.yaml`、45 パス）のうち一部のエンドポイントはサーバー未配線で、公開済みのデプロイ URL はまだ存在しない。進捗の詳細は `REF:docs/plans.md` と `REF:docs/handoff.md` を参照。

## ドキュメント

- ユーザーから見える振る舞いの仕様: `docs/specifications/overview.md`
- API 契約の仕様: `docs/specifications/api.md`（機械可読な契約は `api/openapi.yaml`）
- 実装設計: `docs/designs/overview.md` / `docs/designs/api.md`
- セルフホストの手順: `docs/self-host-guide.md`
- 開発計画と進捗: `docs/plans.md`
- 開発者向け固定情報（規則・環境）: `docs/developers.md`
- 変更履歴・意思決定アーカイブ: `docs/history.md`
- 変更履歴（リリース単位）: `CHANGELOG.md`

## 開発の進め方

このリポジトリは仕様駆動開発のテンプレートを土台にしている。作業前に `docs/developers.md` のドキュメント管理規則と `docs/handoff.md` を読むこと。`scripts/verify.sh` と `python3 docs/script/verify-docs.py` がコミット前の検証として使える。

## 試し方

公開済みのデプロイ URL はまだ存在しないため、以下の手順はローカル環境のみで有効である。ネイティブサーバー（`server/`）はビルドに Rust ツールチェーンが必要で、デプロイされたインスタンスも未提供のため、**フロントエンドはモック API（OpenAPI 契約から応答を生成する Prism）に接続して試す**。

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

起動後、ブラウザで <http://localhost:4321/> を開く。チャット画面（`/collaboration/chat/`）を使う場合は、モック API に加えて別ターミナルで SQLite バックのチャット API も起動する。

```sh
# terminal 3（任意）: チャット API（:4011）。Astro が /api/v1/projects/.../chat/channels 系をこのポートへ转发
npm run chat:dev
```

### 試せることの一覧

モック API は契約の example 応答を返すため、以下は今日実際に試せる範囲である（バックエンド導入後の本番挙動とは異なる）。

- **作品一覧の閲覧**: `/`（モックの `GET /projects` の example データを表示）
- **作品ページ**: `/projects/?projectId=<id>`（任意の projectId で対応）
- **読者ビュー**: `/reading/?projectId=<id>`（本文表示と ETag キャッシュ）
- **公開フロー（ウィザード UI）**: `/publish`（非エンジニア向け 4 ステップの公開ウィザード画面）
- **変更提案・課題**: `/collaboration/proposals/?projectId=<id>`、`/collaboration/issues/?projectId=<id>`
- **チャット**: `/collaboration/chat/?projectId=<id>`（SQLite バックのチャット API を使う場合は別途 `npm run chat:dev` を起動）
- **派生作品一覧**: `/derivatives/?projectId=<id>`
- **点検（診断ジョブ）**: `/inspection/?projectId=<id>`
- **通知センター**: `/notifications/`
- **セットアップ URL 解決**: `/setup/resolve/?u=<URL>`

モック API では契約の example 応答が返るだけで、実データの公開・保存は行われない。ログイン（GitHub OAuth）・実データの永続化・Webhook による再生成はモックでは動作しないため、ネイティブサーバー（`server/`）のビルドと起動が必要である。

## ライセンス

[MIT License](./LICENSE)
