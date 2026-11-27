#!/usr/bin/env bash
# M7 §2.2 外形監視のヘルスチェックスクリプト。
#
# 指定したベース URL の公開ページと hub API の外形（HTTP ステータス）をチェックする。
# 監視対象は「公開ページが応答すること」と「API が応答すること」の外形のみで、
# 内部実装の詳細は調べない（外形監視）。
#
# 用法: ./healthcheck.sh [BASE_URL]
# 既定 BASE_URL は HUB_PUBLIC_BASE 環境変数、未設定なら http://localhost:8080。
#
# 正常時は 0、異常時は 1 を返す。GitHub Actions のモニタリングワークフローで使用。

set -euo pipefail

BASE_URL="${1:-${HUB_PUBLIC_BASE:-http://localhost:8080}}"
BASE_URL="${BASE_URL%/}"

status=0

check() {
    local label="$1" url="$2"
    local code
    # -o /dev/null で本体は見ず、ステータスコードのみ取得（外形監視）。
    code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 10 "$url" 2>/dev/null || echo "000")"
    if [[ "$code" =~ ^2[0-9][0-9]$ ]]; then
        echo "[OK]   ${label}: ${url} -> ${code}"
    else
        echo "[FAIL] ${label}: ${url} -> ${code}" >&2
        status=1
    fi
}

# 公開ページ（ルート）。
check "public-page" "${BASE_URL}/"
# hub API のメタ情報（外形監視のヘルスチェック対象、REF:server/src/lib.rs）。
check "api-meta" "${BASE_URL}/api/v1/meta"

if [[ "$status" -ne 0 ]]; then
    echo "[FAIL] 1 つ以上のチェックが失敗しました" >&2
else
    echo "[OK]   すべて正常"
fi
exit "$status"
