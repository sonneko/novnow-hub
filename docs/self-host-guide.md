# セルフホストガイド（M7 §5.1）

novnow-hub をセルフホスト（自前でデプロイ・運用）するためのガイド。
Workers + D1 を既定構成、VPS + Docker を代替構成とする（仕様: `REF:docs/specifications/m7.md` §2.1、設計: `REF:docs/designs/m7.md` §2.1）。

## 構成の選択

| 構成 | 用途 | 資材 | 手順 |
|---|---|---|---|
| Workers + D1（既定） | Cloudflare Workers 上にサーバー（wasm-bindgen エントリポイント）、D1 をデータ基盤とする | `REF:deploy/workers-d1/wrangler.toml` | このガイド §2 |
| VPS + Docker（代替） | 既存の VPS 上に Docker でデプロイ | `REF:deploy/vps-docker/Dockerfile`、`REF:deploy/vps-docker/docker-compose.yml`、`REF:deploy/vps-docker/nginx/novnow-hub.conf` | このガイド §3 |

## Workers + D1 構成のデプロイ手順（既定）

### アカウント側の操作（必須）

以下の設定は **Cloudflare アカウント側で操作が必要**です。本リポジトリでは行えません。

1. **Cloudflare アカウントの作成**と **Workers**、**D1** サービスの有効化。
2. **Worker の作成**（名前は `novnow-hub` を推奨）と、公開ドメインの割り当て・DNS 設定。`wrangler.toml` の `routes` に記載のパターン（既定 `novnow.example.com/*`）に合わせてドメインを割り当てる。
3. **D1 データベースの作成**（名前例 `novnow-hub-db`）。`database_id` を取得し、`wrangler.toml` の `[[d1_databases]]` の `database_id` プレースホルダを置き換える（D1 への実接続はアカウント側で有効化後、後続タスクとして対応。現時点ではコード側のインターフェース `REF:server/src/ports.rs` の `D1Port` のみを整備）。
4. **GitHub リポジトリの Secrets**（`.github/settings/secrets`）に登録：
   - `CF_API_TOKEN`：Cloudflare API トークン（デプロイ用）。
   - `CF_ACCOUNT_ID`：Cloudflare アカウント ID。
   - `HUB_GITHUB_OAUTH_CLIENT_ID` / `HUB_GITHUB_OAUTH_CLIENT_SECRET` / `HUB_GITHUB_OAUTH_REDIRECT_URI`：GitHub OAuth 設定。
   - `HUB_WEBHOOK_SECRET`：Webhook 署名検証用の HMAC 鍵。

### デプロイの流れ

1. **コード変更を `main` にマージ**する。`.github/workflows/deploy.yml` が `main` への push で起動する。
2. **ビルド**：web（`web/dist`、静的公開ページ）とサーバー（Rust バイナリ、Workers 用 wasm モジュール）をビルドする。
3. **手動承認**：`production` environment の保護ルールで承認者を設定し、デプロイジョブの承認を待つ（設計 §2.1 の「手動承認」）。
4. **デプロイ**：承認後に Worker へデプロイする。`wrangler.toml` の `main` は wasm 出力パス（`server/target/wasm32-unknown-unknown/release/novnow_hub_server.wasm`）、`assets.directory` は `./web/dist`（ワークフローが `web-dist` アーティファクトを `./deploy/workers-d1/web/dist` に展開）を参照する。
5. **検証**：公開ドメインの `/api/v1/meta` と公開ページが 200 で応答することを確認（`REF:deploy/monitoring/healthcheck.sh` で自動化）。

### ロールバック

- デプロイは Git の履歴に依存するため、**前回の正常なマージコミットへ戻す**ことでロールバックできる。
- Worker 側で直前のデプロイバージョンを指名することも可能（Cloudflare のデプロイ履歴を利用）。

### アカウント側の操作待ち

- Cloudflare アカウント・Worker・D1 データベース・ドメイン・Secrets の作成・有効化はアカウント側の操作が必要。D1 への実接続（永続化）はコード側の準備までに留め、実際の接続はアカウント側で D1 バインディングを設定した後、後続タスクで実装する。

## VPS + Docker 構成のデプロイ手順（代替）

### アカウント側の操作（必須）

以下の設定は **VPS のアカウント側で操作が必要**です。本リポジトリでは行えません。

1. **VPS の用意**（クラウドプロバイダ等）と、ドメインの取得・DNS 設定。
2. **Docker と nginx のインストール**（VPS 側で実行）。
3. **TLS 証明書の取得**（Let's Encrypt 等、アカウント側で操作）。
4. **環境変数の設定**：`deploy/vps-docker/.env.example` を参考に `deploy/vps-docker/.env` を作成し、アカウント側で値を入力（`REF:.gitignore` で除外、リポジトリに載せない）。
   - `HUB_PUBLIC_BASE`：公開ベース URL（VPS のドメイン／IP に合わせて設定）。
   - `HUB_GITHUB_OAUTH_CLIENT_ID` / `HUB_GITHUB_OAUTH_CLIENT_SECRET` / `HUB_GITHUB_OAUTH_REDIRECT_URI`：GitHub OAuth 設定。
   - `HUB_WEBHOOK_SECRET`：Webhook 署名検証用の HMAC 鍵。

### デプロイの流れ

1. **リポジトリを VPS にクローン**する。
2. **環境変数を設定**：`deploy/vps-docker/.env` を作成し、アカウント側で値を入力。
3. **イメージをビルド・起動**：
   ```sh
   cd deploy/vps-docker
   docker compose up -d --build
   ```
4. **nginx を設定・起動**：`deploy/vps-docker/nginx/novnow-hub.conf` を `/etc/nginx/sites-available/` に配置し、サイトを有効化。TLS 証明書を `/etc/nginx/certs/` に配置（プレースホルダ `novnow.example.com.fullchain.pem` / `novnow.example.com.key` を実際の証明書に置き換える）。
5. **検証**：公開ドメインの `/api/v1/meta` と公開ページが 200 で応答することを確認（`REF:deploy/monitoring/healthcheck.sh` で自動化）。

### ロールバック

- Docker イメージはタグで管理するため、**直前の正常なイメージタグに戻す**ことでロールバックできる。
- データ永続化は `hub-data` ボリュームを用いる。破損時は `REF:deploy/backup/BACKUP.md` のバックアップから復元。

## 環境変数一覧

| 変数 | 意味 | 既定値 | 設定先 |
|---|---|---|---|
| `HUB_PUBLIC_BASE` | 公開ページのベース URL（`publishedUrl` 生成に使用） | `https://novnow.example.com`（Workers）/ `http://localhost:8080`（Docker） | wrangler.toml `[vars]` / Docker `.env` |
| `HUB_BIND_ADDR` | サーバー待受アドレス | `0.0.0.0` | wrangler.toml `[vars]` / Docker `.env` |
| `HUB_PORT` | 待受ポート | `8080` | wrangler.toml `[vars]` / Docker `.env` |
| `HUB_GITHUB_OAUTH_CLIENT_ID` | GitHub OAuth クライアント ID | （未設定） | GitHub Secrets / Docker `.env` |
| `HUB_GITHUB_OAUTH_CLIENT_SECRET` | GitHub OAuth クライアントシークレット | （未設定） | GitHub Secrets / Docker `.env` |
| `HUB_GITHUB_OAUTH_REDIRECT_URI` | OAuth コールバック URL | （未設定） | GitHub Secrets / Docker `.env` |
| `HUB_WEBHOOK_SECRET` | Webhook 署名検証用 HMAC 鍵 | （未設定） | GitHub Secrets / Docker `.env` |
| `CF_API_TOKEN` | Cloudflare API トークン（デプロイ用） | （未設定） | GitHub Secrets |
| `CF_ACCOUNT_ID` | Cloudflare アカウント ID | （未設定） | GitHub Secrets |

## GitHub 連携の設定

1. **GitHub OAuth App の作成**：GitHub の Developer Settings で OAuth App を作成し、コールバック URL を `HUB_GITHUB_OAUTH_REDIRECT_URI` に設定。Client ID と Client Secret を Secrets に登録。
2. **Webhook 署名鍵の設定**：`HUB_WEBHOOK_SECRET` にランダムな値を生成し、GitHub リポジトリの Webhook 設定と Secrets に同一値を登録。署名は 16 進ダイジェスト形式（`sha256=<hex>`）で検証する。
3. **デプロイ用 Secrets の登録**：`CF_API_TOKEN` と `CF_ACCOUNT_ID` を GitHub リポジトリの Secrets に登録（Workers + D1 構成のデプロイ用）。

## 外形監視（M7 §2.2）

`.github/workflows/monitoring.yml` が 5 分間隔で公開ページと hub API（`GET /api/v1/meta`）の外形チェックを実行し、異常時は GitHub Issue を作成する。`MONITOR_BASE_URL`（GitHub Actions 変数 `vars.MONITOR_BASE_URL`）が未設定、または localhost のプレースホルダ値のときは監視を休止し、ヘルスチェックと Issue 作成を行わず成功終了する。実際の監視対象 URL を設定したときだけ監視が走る（`REF:deploy/monitoring/MONITORING.md`）。

## バックアップと鍵ローテーション（M7 §2.3）

- デプロイ資材のバックアップ：`REF:deploy/backup/backup.sh` と手順 `REF:deploy/backup/BACKUP.md`。データ本体は各構成のデータ基盤（Workers+D1 なら D1 スナップショット、VPS+Docker なら `hub-data` ボリューム）に従いアカウント側で取得。
- 鍵ローテーション：`REF:deploy/backup/rotate-secrets.sh` と手順 `REF:deploy/backup/ROTATION.md`。新しい値の生成 → 各構成の Secrets 更新（アカウント側の操作）→ サービス再起動 → 外形検証。

## 共通の注意

- **秘密はリポジトリに保管しない**。環境変数・Secrets はプレースホルダで、実際の値はデプロイ先のアカウント側で設定する（各手順書に明記）。`.env` は `.gitignore` で除外。
- **デプロイ先のアカウント・ドメイン・外部監視サービス等の有効化はアカウント側の操作が必要**。本リポジトリは手順書・設定ファイル・パイプラインを整えるが、実際の有効化は各サービスの設定画面で行う。
- デプロイパイプラインは GitHub Actions（`REF:.github/workflows/deploy.yml`）を用い、本番環境への適用は**手動承認**（environment protection）を既定とする。
