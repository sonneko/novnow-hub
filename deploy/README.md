# 本番デプロイとインフラ運用（M7 §2）

本ディレクトリは novnow-hub の本番デプロイ（設計 `REF:docs/designs/m7.md` §2）とインフラ運用（外形監視・バックアップ・鍵ローテーション）の資材を置く。

## 構成の選択

仕様 §2.1 に従い、**Workers + D1 を既定**、**VPS + Docker を代替**とする。

| 構成 | 用途 | 資材 | 手順 |
|---|---|---|---|
| Workers + D1（既定） | Cloudflare Workers 上にサーバー、D1 をデータ基盤とする既定構成 | `REF:deploy/workers-d1/wrangler.toml` | `REF:deploy/workers-d1/DEPLOY.md` |
| VPS + Docker（代替） | 従来の VPS 上に Docker でデプロイする代替構成 | `REF:deploy/vps-docker/Dockerfile`、`REF:deploy/vps-docker/docker-compose.yml`、`REF:deploy/vps-docker/nginx/novnow-hub.conf` | `REF:deploy/vps-docker/DEPLOY.md` |

## 外形監視

仕様 §2.2 に従い、公開ページと hub API の外形監視を行う。監視スクリプトと手順は `REF:deploy/monitoring/MONITORING.md`、ヘルスチェックスクリプトは `REF:deploy/monitoring/healthcheck.sh`。定期実行は GitHub Actions のスケジュールワークフロー（`REF:.github/workflows/monitoring.yml`）で行う。

## バックアップと鍵ローテーション

仕様 §2.3 に従い、重要データのバックアップ手順と秘密情報のローテーション手順を整備する。手順は `REF:deploy/backup/BACKUP.md`、スクリプトは `REF:deploy/backup/backup.sh` と `REF:deploy/backup/rotate-secrets.sh`。

## 共通の注意

- **秘密はリポジトリに保管しない**。環境変数・Secrets はプレースホルダで、実際の値はデプロイ先のアカウント側で設定する（各手順書に明記）。
- **デプロイ先のアカウント・ドメイン・外部監視サービス等の有効化はアカウント側の操作が必要**。本リポジトリは手順書・設定ファイル・パイプラインを整えるが、実際の有効化は各サービスの設定画面で行う。
- デプロイパイプラインは GitHub Actions（`REF:.github/workflows/deploy.yml`）を用い、本番環境への適用は**手動承認**（environment protection）を既定とする。
