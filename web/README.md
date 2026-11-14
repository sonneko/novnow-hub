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

- `/`: プロジェクト一覧
- `/projects/?projectId=<id>`: プロジェクト詳細
- `/read/?projectId=<id>`: 読者ビュー

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
npm run tauri -- build
```

Tauri CLI は npm で管理します。IPC コマンドやプラグイン、配布用バンドルは未設定です。
