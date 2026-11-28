# novnow-hub frontend

Astro + Svelte + TypeScript の Web UI です。パッケージ管理には npm を使用します。

## 起動と検証

Node.js 22.12.0 以上を用意し、この `web/` ディレクトリで実行します。

```sh
npm ci
```

開発中は 2 つのターミナルで Prism のモック API と Astro を起動します。

```sh
# terminal 1: OpenAPI 契約から応答を生成するモック API（:4010）
npm run mock

# terminal 2: Web UI（:4321）。/api/v1 をモック API へプロキシします。
npm run dev
```

ブラウザーからは同一オリジンの `/api/v1` を呼び出すため、フロントエンドに localhost の API URL は埋め込みません。Prism は `api/openapi.yaml` を直接読み込み、契約に記載された example とスキーマに基づく応答・リクエスト検証を提供します。バックエンドへ接続するときは `PUBLIC_API_BASE_URL` で API の base URL を指定できます。

チャットの API は SQLite ベースの参照実装（`server/chat-api.mjs`）で提供し、`:4011` で起動します。Astro のプロキシ設定が `/api/v1/projects/{projectId}/chat/channels` 系をこのポートへ转发するため、チャット画面を使う場合はモック API に加えて別ターミナルで `npm run chat:dev` を起動します。

- `/`: プロジェクト一覧
- `/projects/?projectId=<id>`: プロジェクト詳細
- `/reading/?projectId=<id>`: 読者ビュー
- `/collaboration/proposals/?projectId=<id>`: 変更提案一覧
- `/collaboration/issues/?projectId=<id>`: 課題一覧
- `/collaboration/chat/?projectId=<id>`: チャット（`npm run chat:dev` 必須）
- `/derivatives/?projectId=<id>`: 派生ツリー
- `/setup/resolve/?u=<URL>`: セットアップ URL 解決

詳細・読者ページはクエリの `projectId` を使い、API が返す任意の project ID に対応します。

## OpenAPI とテスト

```sh
npm run generate:api # api/openapi.yaml から型を再生成
npm run check:api    # 生成物が契約と一致するか確認
npm run check        # 生成物チェック + Vitest + Astro / Svelte / TypeScript の型チェック
npm test             # Vitest + MSW による API クライアント・ページのテスト
npm run build        # 静的ファイルを dist/ に生成
```

ブラウザー API クライアントは `openapi-typescript` の生成型と `openapi-fetch` を使用します。テストは MSW のインメモリハンドラーで API 応答を差し替えます。

## Tauri 殻

ルートの `src-tauri/` に最小構成のデスクトップシェルを置いています。開発時は
`build.devUrl`、ビルド時は `build.frontendDist`（`web/dist`）を参照します。Rust と
[Tauri 2 の OS 別前提条件](https://v2.tauri.app/start/prerequisites/)を用意したうえで、この
`web/` ディレクトリから実行します。

```sh
npm run tauri -- dev
npm run tauri -- build --bundles
```

Tauri CLI は npm で管理します。IPC コマンド（`session_get`/`session_set`/`session_delete`・`handle_setup_url`）と配布用バンドルは設定済みで、ビルド成果物は CI（`.github/workflows/desktop.yml`、Linux/Win/macOS）で生成します。署名は GitHub Actions の secrets が設定されている場合のみ有効になり、未設定時は未署名でビルドされます（詳細は `REF:docs/self-host-guide.md` と `REF:docs/designs/m6.md` §5）。
