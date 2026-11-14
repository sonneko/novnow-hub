# セッション引き継ぎメモ（Handoff）

本書は開発を始めるにあたって必要な最小限の流動的情報（現在の進捗、作業ブランチ、直近の検証状態）を記録する。毎セッション終了時に更新する。
固定の開発環境構築手順・ドキュメント管理規則・ArenaAgent の仕様・技術的注意点は `REF:docs/developers.md`、今後の実装計画は `REF:docs/plans.md`、完了済みの履歴は `REF:docs/history.md` を参照すること。

---

## 1. 現在の進捗サマリ

<!-- SYNC:START project-status -->
| 区分 | 状態 | 対象範囲 | 参照 |
|---|---|---|---|
| 初期マイルストーン | 完了 | プロジェクトセットアップ・ドキュメント管理規則の策定 | `REF:docs/history.md` |
| M1 仕様策定 | レビュー中 | 仕様書・API 契約（OpenAPI 3.1）・マイルストーン計画の策定 | `REF:docs/plans.md` |
| M2 以降 | 未着手 | フロントエンド基盤・モック・薄いスライスの実装 | `REF:docs/plans.md` |
<!-- SYNC:END project-status -->

---

## 2. 現在のブランチ・PR・直近の作業状態

- **現在のブランチ**: `arena/cb204109-novnow-hub`（`main` 分岐点 `a9945a0`）
- **PR**: [#3](https://github.com/sonneko/novnow-hub/pull/3)（`main` 向け、Ready for review）。CI は docs ジョブが緑、Lint/Test/Build は `api/*`・`docs/*` のみの変更のためスキップされる設計（`.github/workflows/ci.yml` の変更フィルタによる意図的な挙動）
- **現在の状態**: M1 の成果物（仕様書 2 本・設計書 2 本・`api/openapi.yaml`・計画書更新）を実装し、検証を通した。M2 以降の実装には未着手
- **検証**: `python3 docs/script/verify-docs.py` と `openapi-spec-validator` による契約検証が通過することを確認（所要時間: ドキュメント検証は数秒、契約検証は初回のみ依存取得で約 1 分）

---

## 3. 次セッション開始時の手順

1. `/home/user/build/env.sh` が存在しない場合（コンテナ再起動後）は次を実行する（詳細は `REF:docs/developers.md` §2.2）:
   ```bash
   bash scripts/bootstrap-toolchain.sh
   git fetch origin arena/cb204109-novnow-hub
   git reset --mixed FETCH_HEAD
   ```
2. ドキュメント規則を確認する:
   ```bash
   python3 docs/script/verify-docs.py
   ```
3. `REF:docs/plans.md` の M2 から作業を開始する（フロントエンド基盤とモック接続）。M1 がマージされる前に M2 へ進む場合は、マージ後の `main` からの分岐ではなく本ブランチの成果物を前提にすること。
4. 契約ファイル `REF:api/openapi.yaml` を変更する場合は、`openapi-spec-validator` を通し、`REF:docs/specifications/api.md` と `REF:docs/designs/api.md` の対応節も同時に更新すること（仕様を先、設計を後、コードは最後）。
