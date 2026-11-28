# novnow-hub 変更履歴

本ファイルは novnow-hub の変更履歴を記録する。`docs/history.md`（追記専用のアーカイブ）および `git log` と照合し、事実と異なる記載をしない。

## [v0.1.0] - 未リリース（初回リリース準備中）

novnow-hub の OSS としての初回リリースに向けた準備版。M1 から M6 までのマイルストーンが完了し、M7 は §2（デプロイ・監視・バックアップ・鍵ローテーション）・§3（CI 拡張）・§5（ドキュメント整備）・§6（測定結果の記録）が完了、§4 の残項目（CSP・依存ライセンスの棚卸し）と §5.3 の初回リリースは未完了である。リリースの公開（タグ作成・GitHub Release）は別途指示を待つ。

### 追加

- **M7 §2 本番デプロイとインフラ運用**（PR #58、マージコミット `fa55cee`）
  - Workers + D1 を既定構成、VPS + Docker を代替構成とするデプロイ資材を整備。
  - `deploy/workers-d1/wrangler.toml`（Cloudflare Workers 設定・D1 バインディングプレースホルダ）・`deploy/workers-d1/DEPLOY.md`。
  - `deploy/vps-docker/Dockerfile`（多段ビルド）・`deploy/vps-docker/docker-compose.yml`・`deploy/vps-docker/nginx/novnow-hub.conf`・`deploy/vps-docker/DEPLOY.md`・`deploy/vps-docker/.env.example`。
  - `deploy/backup/backup.sh`、`deploy/backup/rotate-secrets.sh` と手順書（バックアップ・鍵ローテーション）。
  - デプロイパイプライン `.github/workflows/deploy.yml`（web とサーバーのビルド → 手動承認ゲート → Workers デプロイ。Cloudflare Secrets 未設定時はスキップ）。
- **M7 §2.2 外形監視**（PR #58）
  - ヘルスチェックスクリプト `deploy/monitoring/healthcheck.sh` と手順 `deploy/monitoring/MONITORING.md`。
  - 定期実行ワークフロー `.github/workflows/monitoring.yml`（既定 5 分間隔、異常時は Issue 作成）。
  - `MONITOR_BASE_URL` 未設定または localhost プレースホルダ時は監視を休止し、成功終了するよう修正（PR #58、マージコミット `fa55cee`）。
- **M7 §2.1 後続：サーバーの Worker ランタイム適応と deploy.yml のパス整合**（PR #59、マージコミット `4e57d0a`）
  - サーバー（Rust + Axum）を Cloudflare Workers (workerd) 上で動作するよう適応。`wasm32-unknown-unknown` 向け `[lib] crate-type = ["rlib", "cdylib"]` と `wasm` feature（`wasm-bindgen` / `js-sys` / `web-sys`）を追加。
  - `server/src/worker.rs` に wasm-bindgen の `fetch` エントリポイントを実装（`GET /api/v1/meta` 対応）。
  - `server/Cargo.toml`、`server/src/main.rs`、`server/src/lib.rs` を `#[cfg(target_arch = "wasm32")]` / `#[cfg(not(target_arch = "wasm32"))]` でネイティブ / wasm に分岐。
  - `server/src/ports.rs` に `D1Port` トレイトを追加（D1 接続のコード側準備。実接続はアカウント側待ち）。
  - `wrangler.toml` の `main` を wasm 出力パスに変更し、`assets.directory` を `./web/dist` に変更。
  - `deploy.yml` の `workingDirectory` を `./deploy/workers-d1` に変更し、`web-dist` アーティファクトを `./deploy/workers-d1/web/dist` に展開して wrangler.toml の `assets.directory` と整合。
- **M7 §3 CI 拡張**（PR #56）
  - 契約互換性検査（独自差分チェックスクリプト `docs/script/check-openapi-compat.py`）。
  - `cargo audit`（真の脆弱性のみ失敗）・`npm audit`（production 依存のみ失敗）・カバレッジゲート。
- **M7 §4 セキュリティレビュー**（PR #54/#55）
  - トークン暗号化保管（OS キーチェーン主back、利用不可時は AES-256-GCM 暗号化ファイルへフォールバック、マシン固有エントロピーを鍵導出に混入）。
  - zip 取込のパス traversal 対策、CSP、権限判定。

### 変更

- **M6 マルチプラットフォーム**（PR #52）
  - Tauri v2 による Web と同一コードベースのデスクトップビルド（Linux/Win/macOS）と配布パイプライン `.github/workflows/desktop.yml`（未署名インストーラ）。
  - PWA 化（`manifest.json`・Service Worker オフラインシェル・インストール済みアプリとしてのオフライン閲覧）。
  - Tauri secure storage へのセッション保管抽象化、カスタムスキーム `novnow://` のセットアップ URL 受信。
- **M5 novnow 特化機能**（PR #49/#50 等）
  - セットアップ URL 発行・解決（PR #12）。
  - 派生（DerivationPort・GitHub Fork 連携・派生レコード・派生ツリー UI）。
  - novnow 診断・エクスポートの非同期ジョブ基盤（JobPort）と「点検」タブ。
  - 変更提案差分への novnow 診断差分の付与、公開前検証の本格化（`/verify`・`/verify/latest` 配線）。
- **M4 コラボレーション**（PR #44〜#48）
  - 変更提案・課題・自動マージの GitHub 実装と契約テスト、HTTP ルート配線。
  - 通知センター・未読バッジ・通知ファンアウト。
  - GitHub Webhook 受信（HMAC-SHA256 署名検証、16 進ダイジェスト形式）による公開ページ再生成とキャッシュ失効。
- **M3 公開と閲覧**（PR #8/#39/#41/#42）
  - バックエンド骨格（Rust + Axum、新規 crate `server/`）、GitHub アダプター最小実装。
  - 公開フロー（zip 取込 → novnow 軽量検証 → 保管単位作成 → hub 登録 → 公開ページ生成）。
  - 読者ビュー・公開ウィザード UI・プロジェクト一覧・検索（カーソルページネーション）。
- **M2 フロントエンド基盤**（PR #38 等）
  - Astro + Svelte + TypeScript（strict）、API クライアント契約生成、OpenAPI モック、アプリシェル、CI 拡張（契約スキーマ検証 + Prism モックのスモークテスト）。
- **M1 仕様策定**
  - 仕様書・API 契約（OpenAPI 3.1）・マイルストーン計画の策定。ドキュメント管理規則の策定。

### 削除

- なし（M1〜M7 で破壊的な削除は行っていない。API 契約 `api/openapi.yaml` は不変）。

### 非推奨

- なし。

## [0.0.x] - 開発中（リリース前）

初期開発期間。API 契約・仕様書・設計書の策定、プロトタイプ実装。

---

## バージョン表記

- セマンティックバージョニング（SemVer）に従い、`v<major>.<minor>.<patch>` の形式で表記する。
- 初回リリースのバージョンは `v0.1.0`（OSS としての初回リリース、API 契約 `info.version` は `1.0.0` と不変）。

## タグ計画

- 初回リリース時に `git tag v0.1.0` を PR #61 マージ後（`main` = `c8b97ca`）の最新コミットに打ち、GitHub Release を作成する。
- タグの作成・GitHub Release の作成・リリースの公開は本リポジトリでは行わず、別途指示を待つ。
- 以降のリリースは `v0.1.1`（パッチ）・`v0.2.0`（マイナー）等、変更の程度に応じて判断する。

## 参考

- 詳細な変更履歴と意思決定は `docs/history.md`（追記専用）を参照。
- 現在の開発状況は `docs/handoff.md`、実装計画は `docs/plans.md` を参照。
