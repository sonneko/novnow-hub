# リリースノート草案：novnow-hub v0.1.0（M7 §5.3 初回リリース準備）

本文書は novnow-hub v0.1.0 のリリースノート草案である。タグの作成・GitHub Release の作成・リリースの公開は行わず、別途指示を待つ。

## バージョン

- **v0.1.0**（初回リリース）
- API 契約 `api/openapi.yaml` の `info.version` は `1.0.0` と不変（契約の正）。

## 対象コミット

- PR #61 マージ後の `main` 最新コミット（`c8b97ca`）に `git tag v0.1.0` を打つ計画。

## 概要

novnow-hub の OSS としての初回リリース。M1 から M6 までのマイルストーンが完了し、M7 は §2（デプロイ・監視・バックアップ・鍵ローテーション）・§3（CI 拡張）・§5（ドキュメント整備）・§6（測定結果の記録）が完了、§4 の残項目（CSP・依存ライセンスの棚卸し）と §5.3 の初回リリース自体は未完了である。Phase 1（外部基盤を土台とした提供形態）のスコープとして、GitHub を保管基盤としたプロジェクトの公開・閲覧・コラボレーション・派生・診断・エクスポートと、本番デプロイ・外形監視・CI・セキュリティレビューを備える。

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
4. **契約にあるが未配線のエンドポイントがある**：`GET /setup/resolve`・`GET /projects/{id}/setup`・`GET /projects/{id}/reading`・`GET /projects/{id}/tree`・`GET /projects/{id}/file`・`POST /projects/{id}/commit`・`GET /projects/{id}/releases`・`GET /users/*`・`GET /search/users`・`GET`/`PATCH /notifications/settings` は `api/openapi.yaml` に定義済みだが、ネイティブサーバー（`server/src/lib.rs` の `build_router`）に未配線である（判断: `REF:docs/handoff.md` §4 判断 27）。フロントエンドは接続済みだが、サーバーに接続すると該当機能は 404 になる。
5. **Worker（wasm）ランタイムが応答するのは `/api/v1/meta` のみ**：`REF:server/src/worker.rs` は `GET /api/v1/meta` のみを実装し、他の API は 404。公開ページは wrangler の `assets`（`web/dist`）が配信する。VPS + Docker 構成では静的公開ページの配信経路が未整備である。
6. **性能目標・可用性・コストは未実測**：LCP・API p95・月間可用性・運用コストはいずれも実測できておらず、推定にとどまる（`REF:docs/performance-measurement.md`）。

## タグ計画

- `git tag v0.1.0` を PR #61 マージ後の `main` 最新コミット（`c8b97ca`）に打つ。
- タグの作成・GitHub Release の作成・リリースの公開は行わず、別途指示を待つ。
- 以降のリリースはセマンティックバージョニングに従い、`v0.1.1`（パッチ）・`v0.2.0`（マイナー）等とする（`CHANGELOG.md` 参照）。

## ドキュメント

- セルフホストガイド：`REF:docs/self-host-guide.md`
- 運営ポリシー：`REF:docs/operating-policy.md`
- 変更履歴：`REF:CHANGELOG.md`
- 開発状況：`REF:docs/handoff.md`、実装計画：`REF:docs/plans.md`、変更履歴アーカイブ：`REF:docs/history.md`
