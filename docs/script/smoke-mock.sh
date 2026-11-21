#!/usr/bin/env bash
# モックのスモークテスト。
#
# Prism モック（OpenAPI から起動）を立ち上げ、読者ビュー・プロジェクトページが
# 接続する主要なパスへ GET し、2xx が返ることを確認する。
# 対象パスは docs/plans.md の M2 完了条件「モックに接続した読者ビューと
# プロジェクトページがローカルで動くこと」と整合する。
#
# 使用法: bash docs/script/smoke-mock.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WEB_DIR="$REPO_ROOT/web"
MOCK_HOST="127.0.0.1"
MOCK_PORT="${MOCK_PORT:-4010}"
SPEC="$REPO_ROOT/api/openapi.yaml"

cd "$WEB_DIR"

if [ ! -d node_modules ] || [ ! -x node_modules/.bin/prism ]; then
  echo "Prism をインストールします（npm ci）…"
  npm ci --no-audit --no-fund
fi

echo "Prism モックを起動します（$SPEC, ${MOCK_HOST}:${MOCK_PORT}）…"
node_modules/.bin/prism mock "$SPEC" --host "$MOCK_HOST" --port "$MOCK_PORT" > /tmp/smoke-prism.log 2>&1 &
MOCK_PID=$!
trap 'kill "$MOCK_PID" 2>/dev/null || true' EXIT

echo "モックの起動を待機します…"
ready=0
for _ in $(seq 1 30); do
  if curl -sf -o /dev/null "http://$MOCK_HOST:$MOCK_PORT/meta"; then
    ready=1
    break
  fi
  sleep 1
done
if [ "$ready" -ne 1 ]; then
  echo "::error::Prism モックが起動しませんでした（ログ: /tmp/smoke-prism.log）"
  tail -20 /tmp/smoke-prism.log
  exit 1
fi

# スモーク対象: 読者ビュー（reading）・プロジェクトページ（project / tree / setup）が接続するパス + 基本パス。
PATHS=(
  "/meta"
  "/projects/p-001"
  "/projects/p-001/reading"
  "/projects/p-001/tree"
  "/projects/p-001/setup"
)

failures=0
for path in "${PATHS[@]}"; do
  status="$(curl -s -o /dev/null -w '%{http_code}' "http://$MOCK_HOST:$MOCK_PORT$path")"
  case "$status" in
    2*) echo "smoke: GET $path -> $status OK" ;;
    *) echo "::error::smoke: GET $path -> $status（2xx が返りませんでした）"; failures=$((failures + 1)) ;;
  esac
done

if [ "$failures" -ne 0 ]; then
  echo "::error::スモークテスト失敗（${failures} 件のパスが 2xx を返しませんでした）"
  exit 1
fi
echo "smoke: 全パスが 2xx を返しました（${#PATHS[@]} パス）"
