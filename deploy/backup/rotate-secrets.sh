#!/usr/bin/env bash
# M7 §2.3 鍵ローテーションのスクリプト。
#
# 指定した秘密（環境変数名）を新しい値でローテーションする。
# ローテーション手順:
#   1. 新しい値を生成（--generate でランダム値）。
#   2. デプロイ先の Secrets を更新（アカウント側の操作）。
#   3. サービスを再起動して新値を反映。
#
# 秘密はリポジトリに載せない。本スクリプトは手順の補助であり、
# 実際の更新はアカウント側で行う。
#
# 用法:
#   ./rotate-secrets.sh --name HUB_WEBHOOK_SECRET --generate
#   ./rotate-secrets.sh --name HUB_WEBHOOK_SECRET --value <NEW_VALUE>

set -euo pipefail

NAME=""
VALUE=""
GENERATE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --name) NAME="$2"; shift 2 ;;
        --value) VALUE="$2"; shift 2 ;;
        --generate) GENERATE=1; shift ;;
        *) echo "不明な引数: $1" >&2; exit 1 ;;
    esac
done

if [[ -z "$NAME" ]]; then
    echo "--name でローテーションする秘密名を指定してください" >&2
    exit 1
fi

if [[ "$GENERATE" -eq 1 ]]; then
    # ランダム値を生成（32 文字の英数）。
    VALUE="$(LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c 32)"
    echo "生成した新しい値（失念しないよう账户側で記録）: ${VALUE}"
else
    if [[ -z "$VALUE" ]]; then
        echo "--generate か --value で新しい値を指定してください" >&2
        exit 1
    fi
fi

echo "==> 秘密 ${NAME} をローテーションします"
echo "    新しい値はアカウント側で Secrets に設定してください（リポジトリには載せない）"
echo "    反映にはサービスの再起動が必要です（各構成の手順書を参照）"
echo "[OK] ローテーション手順の案内を完了"
