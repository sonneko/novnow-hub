# Workers + D1 デプロイ手順（既定構成、M7 §2.1）

本手順は novnow-hub を **Cloudflare Workers + D1** に本番デプロイする手順。デプロイパイプラインは GitHub Actions（`REF:.github/workflows/deploy.yml`）を用い、本番環境への適用は**手動承認**を既定とする。

## 前提（アカウント側の操作）

以下の設定は **Cloudflare アカウント側で操作が必要**。本リポジトリでは行えない。

1. **Cloudflare アカウントの作成**と **Workers**・**D1** サービスの有効化。
2. **Worker の作成**（名前は `novnow-hub` を推奨）と、公開ドメインの割り当て・DNS 設定。
3. **D1 データベースの作成**（名前例 `novnow-hub-db`）。`database_id` を取得。
4. **Secrets の設定**（リポジトリには載せない）：
   - `HUB_GITHUB_OAUTH_CLIENT_ID` / `HUB_GITHUB_OAUTH_CLIENT_SECRET` / `HUB_GITHUB_OAUTH_REDIRECT_URI`（GitHub OAuth 設定）
   - `HUB_WEBHOOK_SECRET`（Webhook 署名検証用の HMAC 鍵）
5. **GitHub リポジトリの Secrets**（`.github/settings/secrets`）にデプロイ用の Cloudflare API トークン（`CF_API_TOKEN`）を登録。

## デプロイの流れ

1. **コード変更を main にマージ**。`REF:.github/workflows/deploy.yml` が `main` への push で起動する。
2. **ビルド**: web（`web/dist`）とサーバー（Rust バイナリ）をビルドする。
3. **手動承認**: `production` environment の保護ルールで承認者を設定し、デプロイジョブの承認を待つ（設計 §2.1 の「手動承認」）。
4. **デプロイ**: 承認後に Worker へデプロイし、D1 に接続する。
5. **検証**: 公開ドメインの `/api/v1/meta` と公開ページが 200 で応答することを確認（`REF:deploy/monitoring/healthcheck.sh` で自動化）。

## ロールバック

- デプロイは Git の履歴に依存するため、**前回の正常なマージコミットへ戻す**ことでロールバックできる。
- Worker 側で直前のデプロイバージョンを指名することも可能（Cloudflare のデプロイ履歴を利用）。

## 補足（判断記録）

- **サーバーの Worker ランタイム適応**: 現行のサーバー（Rust + Axum、インメモリストア）を Cloudflare Workers の workerd 上で実行するには、ランタイム適応（ファイルシステム非依存化等）が必要。これは後続タスクとし、本手順ではパイプラインと設定を整える。判断は `REF:docs/handoff.md` §4 判断 21 に記録。
- **D1 への実接続**: 現行はインメモリストア＋GitHub を主backとする。D1 への永続化実装は後続タスク（判断 21）。
- ドメイン・アカウント・Secrets の有効化はアカウント側の操作。本リポジトリは手順と設定のみを整える。
