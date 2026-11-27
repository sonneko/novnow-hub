# 鍵ローテーション（M7 §2.3）

仕様 §2.3 に従い、novnow-hub の秘密情報（認証情報・署名鍵等）のローテーション手順を整備する。

## 対象となる秘密

| 秘密 | 用途 | 設定先 |
|---|---|---|
| `HUB_GITHUB_OAUTH_CLIENT_SECRET` | GitHub OAuth のクライアント秘密 | 各構成の Secrets |
| `HUB_WEBHOOK_SECRET` | Webhook 署名検証用の HMAC 鍵 | 各構成の Secrets |

## ローテーション手順

### 1. 新しい値の生成

```sh
./deploy/backup/rotate-secrets.sh --name HUB_WEBHOOK_SECRET --generate
```

- スクリプトがランダムな新しい値を生成して表示する。
- **新しい値はリポジトリに載せず、アカウント側で安全に記録する。**

### 2. デプロイ先の Secrets を更新（アカウント側の操作）

- **Workers + D1**: Cloudflare の Secrets を更新（`wrangler secret put` 等、アカウント側で操作）。
- **VPS + Docker**: `deploy/vps-docker/.env` を更新（アカウント側で操作）。
- **GitHub Actions**: リポジトリ Secrets を更新（アカウント側で操作）。

### 3. サービスの再起動

- 各構成の手順に従い、サービスを再起動して新値を反映する。
- **Workers + D1**: Worker を再デプロイ。
- **VPS + Docker**: `docker compose up -d` で再起動。

### 4. 検証

- ローテーション後、`REF:deploy/monitoring/healthcheck.sh` で外形が正常か検証する。
- 旧値での認証が失效することを確認（必要に応じて）。

## 判断記録

- **ローテーション対象の絞り方**: 認証情報（GitHub OAuth）と署名鍵（Webhook）を対象とし、ローテーション済みの値は以前の値を無効化する手順を含める。判断は `REF:docs/handoff.md` §4 判断 25 に記録。
- 秘密の実際の値はリポジトリに載せず、プレースホルダと手順で対応する。
