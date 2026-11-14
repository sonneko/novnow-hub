# novnow-hub

novnow 形式の作品を公開・共有する Web プラットフォーム。非エンジニアが novnow 形式のファイルをサッと Web 公開でき、二次創作者が URL ひとつでアプリ内に自動セットアップしたり、URL だけでセットアップ済みの novnow をブラウザで開いたりできる。GitHub のように変更提案（PR 相当）+ 課題（Issue 相当）+ 自動マージ + チャットが使え、novnow 特化の機能も提供する。

- ソフトウェアとして OSS 公開（MIT License）を前提とし、サーバー費用は最小限に抑える設計
- フロントエンド: Astro + Svelte + TypeScript（Tauri でマルチプラットフォーム化）
- Phase 1 は GitHub を土台（公開プロジェクト＝GitHub リポジトリ）。ただし GitHub 依存はポート／アダプターの背後に隔離され、API 契約とフロントエンドは GitHub を知らない（`REF:docs/specifications/api.md` と `REF:docs/handoff.md` 参照）
- バックエンドは別途開発（Rust + Axum + SQLite 等を候補）。フロントエンドは API 契約（OpenAPI 3.1）のみに依存する

## 現在のステータス

M1（仕様策定・API 契約）が完了し、M2（フロントエンド基盤とモック接続）以降は未着手。詳細は `REF:docs/plans.md` を参照。

## ドキュメント

- ユーザーから見える振る舞いの仕様: `docs/specifications/overview.md`
- API 契約の仕様: `docs/specifications/api.md`（機械可読な契約は `api/openapi.yaml`）
- 実装設計: `docs/designs/overview.md` / `docs/designs/api.md`
- 開発計画と進捗: `docs/plans.md`
- 開発者向け固定情報（規則・環境）: `docs/developers.md`
- 変更履歴・意思決定アーカイブ: `docs/history.md`

## 開発の進め方

このリポジトリは仕様駆動開発のテンプレートを土台にしている。作業前に `docs/developers.md` のドキュメント管理規則と `docs/handoff.md` を読むこと。`scripts/verify.sh` と `python3 docs/script/verify-docs.py` がコミット前の検証として使える。

## ライセンス

[MIT License](./LICENSE)
