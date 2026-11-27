# 外形監視（M7 §2.2）

仕様 §2.2 に従い、novnow-hub の公開ページと hub API の外形監視を行う。監視対象は「応答しているか」という外形（HTTP ステータス）であり、内部実装の詳細は調べない。

## 監視対象

| 対象 | エンドポイント | 監視内容 |
|---|---|---|
| 公開ページ | `GET /` | 200 で応答すること |
| hub API メタ情報 | `GET /api/v1/meta` | 200 で応答すること（REF:server/src/lib.rs のルート配線） |

## 監視方法

### ヘルスチェックスクリプト

`REF:deploy/monitoring/healthcheck.sh` が上記 2 点をチェックする。

```sh
./deploy/monitoring/healthcheck.sh https://novnow.example.com
```

- 正常時は 0、異常時は 1 を返す。
- GitHub Actions のモニタリングワークフロー（`REF:.github/workflows/monitoring.yml`）で定期実行する。

### 定期実行

- GitHub Actions のスケジュール（既定 5 分間隔）で定期的に実行し、異常時は Issue を作成する想定。
- 外部監視サービス（StatusPage 等）への連携は任意。外部サービスの有効化はアカウント側の操作。

## 判断記録

- **監視範囲は外形のみ**: 内部のステータス・パフォーマンス計測は §6（性能実測）と区別し、ここでは「応答しているか」のみを監視する。判断は `REF:docs/handoff.md` §4 判断 23 に記録。
