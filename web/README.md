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
