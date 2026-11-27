# VPS + Docker デプロイ手順（代替構成、M7 §2.1）

本手順は novnow-hub を **VPS 上の Docker** に本番デプロイする代替構成の手順。既定構成（Workers+D1）と比べ、既存の VPS 運用を流用できる利点がある。

## 前提（アカウント側の操作）

以下の設定は **VPS のアカウント側で操作が必要**。本リポジトリでは行えない。

1. **VPS の用意**（クラウドプロバイダ等）と、ドメインの取得・DNS 設定。
2. **Docker と nginx のインストール**（VPS 側で実行）。
3. **TLS 証明書の取得**（Let's Encrypt 等、アカウント側で操作）。
4. **秘密の設定**（リポジトリには載せない）：
   - `deploy/vps-docker/.env` を `.env.example` を参考に作成（`REF:.gitignore` で除外）。
   - `HUB_GITHUB_OAUTH_CLIENT_ID` / `HUB_GITHUB_OAUTH_CLIENT_SECRET` / `HUB_GITHUB_OAUTH_REDIRECT_URI`
   - `HUB_WEBHOOK_SECRET`

## デプロイの流れ

1. **リポジトリを VPS にクローン**。
2. **環境変数を設定**: `deploy/vps-docker/.env` を作成し、アカウント側で値を入力。
3. **イメージをビルド・起動**:
   ```sh
   cd deploy/vps-docker
   docker compose up -d --build
   ```
4. **nginx を設定・起動**: `deploy/vps-docker/nginx/novnow-hub.conf` を `/etc/nginx/sites-available/` に配置し、サーブレットを有効化。TTL 証明書を配置。
5. **検証**: 公開ドメインの `/api/v1/meta` と公開ページが 200 で応答することを確認（`REF:deploy/monitoring/healthcheck.sh` で自動化）。

## ロールバック

- Docker イメージはタグで管理するため、**直前の正常なイメージタグに戻す**ことでロールバックできる。
- データ永続化は `hub-data` ボリュームを用いる。破損時は `REF:deploy/backup/BACKUP.md` のバックアップから復元。

## 補足（判断記録）

- **nginx 逆プロキシ**: VPS 構成では nginx で TLS 終端と反向代理を行い、サーバーは HTTP で待受する。判断は `REF:docs/handoff.md` §4 判断 22 に記録。
- ドメイン・証明書・秘密の設定はアカウント側の操作。本リポジトリは手順と設定のみを整える。
