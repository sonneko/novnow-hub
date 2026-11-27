#!/usr/bin/env bash
# M7 §2.3 バックアップのスクリプト。
#
# 重要データのバックアップを作成する。対象は:
#   - インメモリストアのスナップショット（必要に応じて）
#   - 設定ファイル（nginx 設定等のプロビジョニング資材）
#
# 出力は gzip 圧縮した tar ファイル。保存先は BACKUP_DIR（既定 ./backups）。
# リポジトリにはバックアップ本体を載せない（OUT は .gitignore 対象）。
#
# 用法: ./backup.sh

set -euo pipefail

# バックアップ保存先（既定は deploy/backup/backups、.gitignore 対象）。
BACKUP_DIR="${BACKUP_DIR:-$(dirname "$0")/backups}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
DEST="${BACKUP_DIR}/novnow-hub-backup-${STAMP}.tar.gz"

mkdir -p "$BACKUP_DIR"

# バックアップ対象: デプロイ資材（プロビジョニング情報）。
# データ本体（インメモリストア）は実データではなく、必要に応じて追加する。
# 実際のデータバックアップは各構成の手順書を参照。
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

tar -czf "$DEST" \
    -C "$REPO_ROOT" \
    deploy \
    .github/workflows/deploy.yml

echo "[OK] バックアップを作成しました: ${DEST}"
