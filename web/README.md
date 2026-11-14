# novnow-hub frontend

Astro + Svelte + TypeScript の最小スキャフォールドです。npm を使用します。

## 起動と検証

Node.js 22.12.0 以上を用意し、この `web/` ディレクトリで実行します。

```sh
npm ci
npm run dev
```

- `npm run dev`: 開発サーバーを起動します。
- `npm run build`: 静的ファイルを `dist/` に生成します。
- `npm run check`: Astro / Svelte / TypeScript の型チェックを実行します。
- `npm run preview`: ビルド成果物をローカルで確認します。

## Tauri 殻

ルートの `src-tauri/` に最小構成のデスクトップシェルを置いています。開発時は
`build.devUrl`、ビルド時は `build.frontendDist`（`web/dist`）を参照します。
Rust と [Tauri 2 の OS 別前提条件](https://v2.tauri.app/start/prerequisites/)を
用意したうえで、この `web/` ディレクトリから実行します。

```sh
npm run tauri -- dev
npm run tauri -- build
```

Tauri CLI は npm で管理します。IPC コマンドやプラグイン、配布用バンドルは未設定です。
