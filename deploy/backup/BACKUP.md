# バックアップと復元（M7 §2.3）

仕様 §2.3 に従い、novnow-hub の重要データのバックアップ手順を整備する。

## バックアップ対象

| 対象 | 内容 | 備考 |
|---|---|---|
| デプロイ資材 | `deploy/` と `.github/workflows/deploy.yml` | プロビジョニング情報。`REF:deploy/backup/backup.sh` で取得 |
| データ（Workers+D1） | D1 データベースのダンプ | Cloudflare 側の D1 スナップショット機能利用（アカウント側の操作） |
| データ（VPS+Docker） | `hub-data` ボリューム | `docker run --rm -v hub-data:/data alpine tar czf - /data > backup.tgz`（アカウント側の操作） |

## バックアップ手順

### デプロイ資材

```sh
./deploy/backup/backup.sh
```

- 既定で `deploy/backup/backups/` に gzip 圧縮した tar を作成。
- バックアップ本体はリポジトリに載せない（`REF:.gitignore` 対象）。

### データ（各構成）

- **Workers + D1**: Cloudflare ダッシュボードの D1 スナップショット機能で取得（アカウント側の操作）。
- **VPS + Docker**: `hub-data` ボリュームのダンプを VPS 側で取得（アカウント側の操作）。

## 復元手順

1. バックアップファイルを復元先のアカウント側で用意。
2. デプロイ資材はリポジトリの `deploy/` に展開。
3. データは各構成の手順に従い復元（D1 のスナップショット復元 / ボリュームの差し替え）。
4. 復元後、`REF:deploy/monitoring/healthcheck.sh` で外形が正常か検証する。

## 判断記録

- **バックアップ対象の範囲**: 現行はインメモリストア＋GitHub を主backとするため、実データの永続化対象は各構成のデータ基盤（D1 / ボリューム）に絞る。デプロイ資材は付随的にバックアップする。判断は `REF:docs/handoff.md` §4 判断 24 に記録。
