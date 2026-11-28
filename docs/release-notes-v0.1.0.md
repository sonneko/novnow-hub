# リリースノート草案：novnow-hub v0.1.0（M7 §5.3 初回リリース準備）

本文書は novnow-hub v0.1.0 のリリースノート草案である。タグの作成・GitHub Release の作成・リリースの公開は行わず、別途指示を待つ。

## バージョン

- **v0.1.0**（初回リリース）
- API 契約 `api/openapi.yaml` の `info.version` は `1.0.0` と不変（契約の正）。

## 対象コミット

- PR #60 マージ後の `main` 最新コミットに `git tag v0.1.0` を打つ計画。

## 概要

novnow-hub の OSS としての初回リリース。M1 から M7 までのマイルストーンが完了し、OSS として一般公開できる状態。Phase 1（外部基盤を土台とした提供形態）のスコープとして、GitHub を保管基盤としたプロジェクトの公開・閲覧・コラボレーション・派生・診断・エクスポートと、本番デプロイ・外形監視・CI・セキュリティレビューを備える。

## 主な変更

### M7 運用・品質・OSS 公開準備

- **§2 本番デプロイとインフラ運用**（PR #58・#59）
  - Workers + D1 を既定構成、VPS + Docker を代替構成とするデプロイ資材を整備（`deploy/`）。
  - サーバー（Rust + Axum）を Cloudflare Workers (workerd) 上で動作するよう適応（`wasm32-unknown-unknown` / wasm-bindgen エントリポイント）。
  - `deploy.yml` と `wrangler.toml` のパス整合（`workingDirectory: ./deploy/workers-d1`、`web-dist` アーティファクトを `./deploy/workers-d1/web/dist` に展開）。
  - 外形監視（5 分間隔、`MONITOR_BASE_URL` 未設定時は監視休止）、バックアップ、鍵ローテーション。
  - Cloudflare アカウント・Worker・D1 データベース・ドメイン・Secrets の作成・有効化はアカウント側の操作が必要（`deploy/workers-d1/DEPLOY.md` 参照）。
- **§3 CI 拡張**（PR #56）
  - 契約互換性検査、`cargo audit`／`npm audit`、カバレッジゲート。
- **§4 セキュリティレビュー**（PR #54/#55）
  - トークン暗号化保管、zip 取込・CSP・権限判定のレビュー。

### M1〜M6

- M1 仕様策定（API 契約 OpenAPI 3.1、ポート・アダプター設計、GitHub 概念の語彙排除）。
- M2 フロントエンド基盤（Astro + Svelte + TypeScript、API 型生成、OpenAPI モック）。
- M3 公開と閲覧（バックエンド骨格 Rust + Axum、公開フロー、読者ビュー、プロジェクト一覧・検索）。
- M4 コラボレーション（変更提案・課題・自動マージ、通知センター、Webhook 受信）。
- M5 novnow 特化機能（セットアップ URL、派生ツリー、JobPort 診断・エクスポート）。
- M6 マルチプラットフォーム（Tauri v2 デスクトップビルド、PWA）。

## 既知の制限事項（リリース時点）

1. **D1 への実接続は未実施**：現行のサーバーはインメモリストア＋GitHub を主backとする。D1 への永続化は後続タスク（判断: `REF:docs/handoff.md` §4 判断 26）。Workers + D1 構成で D1 を使用するには、アカウント側で D1 データベースを作成し `wrangler.toml` の `[[d1_databases]]` にバインディングを設定した後、サーバー側の実装が必要。
2. **Cloudflare アカウント側の操作待ち**：Worker の作成・ドメイン割り当て・D1 データベース作成・Secrets 設定（`CF_API_TOKEN`・`CF_ACCOUNT_ID`・OAuth Client ID/Secret 等）はアカウント側で行う必要がある。未設定時はデプロイジョブがスキップされる（notice を出力して成功終了）。
3. **Tauri デスクトップビルドは未署名インストーラ**：署名証明書がこの開発環境で用意できないため、Phase 1 の既定配布方式は CI で生成した未署名インストーラ。署名は後続タスク（判断: `REF:docs/handoff.md` §4 判断 15）。

## タグ計画

- `git tag v0.1.0` を PR #60 マージ後の `main` 最新コミットに打つ。
- タグの作成・GitHub Release の作成・リリースの公開は行わず、別途指示を待つ。
- 以降のリリースはセマンティックバージョニングに従い、`v0.1.1`（パッチ）・`v0.2.0`（マイナー）等とする（`CHANGELOG.md` 参照）。

## ドキュメント

- セルフホストガイド：`REF:docs/self-host-guide.md`
- 運営ポリシー：`REF:docs/operating-policy.md`
- 変更履歴：`REF:CHANGELOG.md`
- 開発状況：`REF:docs/handoff.md`、実装計画：`REF:docs/plans.md`、変更履歴アーカイブ：`REF:docs/history.md`
