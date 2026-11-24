# セッション引き継ぎメモ（Handoff）

本書は開発を始めるにあたって必要な最小限の流動的情報（現在の進捗、作業ブランチ、直近の検証状態）を記録する。毎セッション終了時に更新する。
固定の開発環境構築手順・ドキュメント管理規則・ArenaAgent の仕様・技術的注意点は `REF:docs/developers.md`、今後の実装計画は `REF:docs/plans.md`、完了済みの履歴は `REF:docs/history.md` を参照すること。

---

## 1. 現在の進捗サマリ

<!-- SYNC:START project-status -->
| 区分 | 状態 | 対象範囲 | 参照 |
|---|---|---|---|
| 初期マイルストーン | 完了 | プロジェクトセットアップ・ドキュメント管理規則の策定 | `REF:docs/history.md` |
| M1 仕様策定 | 完了 | 仕様書・API 契約（OpenAPI 3.1）・マイルストーン計画の策定 | `REF:docs/plans.md#m1-specs` |
| M2 フロントエンド基盤 | 完了 | Astro／Svelte、API 型生成、OpenAPI モック、アプリシェル、CI 拡張（契約スキーマ検証 + Prism モックのスモークテストを docs ジョブに追加済み） | `REF:docs/plans.md#m2-frontend-mock` |
| M3 公開と閲覧 | 進行中 | 読者ビュー・公開ページの静的シェル事前描画・ETag キャッシュ・公開ウィザード UI・バックエンド骨格（Rust + Axum）・GitHub アダプター最小実装・公開フロー本体（multipart 取込・novnow 軽量検証・保管単位作成・hub 登録・公開ページ生成）を実装。検索は未完了 | `REF:docs/plans.md#m3-thin-slice` |
| M4 コラボレーション | 進行中 | Proposal／Issue UI と SQLite ChatPort 参照実装・チャット UI を実装。自動マージ・通知等は未完了 | `REF:docs/plans.md#m4-collaboration` |
| M5 novnow 特化機能 | 進行中 | セットアップ URL 発行・解決を実装。派生・診断等は未完了 | `REF:docs/plans.md#m5-novnow-features` |
<!-- SYNC:END project-status -->

---

## 2. 現在のブランチ・PR・直近の作業状態

- **現在のブランチ**: `arena/3f2aec93-novnow-hub`（`main` 分岐点 `c279c6493c30f613183cf5007081e0d76642c06c`、PR #41 のマージコミット。直近の `main` は PR #41 マージ時点）
- **PR**: 直近のマージ済み PR は **#40（M3 zip 取込モジュール）・#41（zip 取込モジュール、マージコミット `c279c64`）**。本ブランチは M3 の公開フロー本体（multipart 取込・novnow 軽量検証・保管単位作成・hub 登録・公開ページ生成）を実装済み（PR 作成待ち）。マージはユーザーの指示を待つ。
- **Issues #22–#33 のクローズ**: 全 12 Issue について `main` @ `547e08a` の現状を実行で検証し、解消済みとしてすべてクローズ済み（検証日 2026-11-21）。詳細は各 Issue コメントを参照。#22（example 応答）・#23（If-Match+409）・#24（Idempotency-Key 12 操作）・#25（RateLimit ヘッダ）・#26（approve は verdict 3 値）・#27（comments GET 仕様追補）・#28（ChatPort 操作名一致）・#29（contractVersion/info.version 一致）・#30（diagnostics 認証要）・#31（visibility 3 値）・#32（読者ビュー文字サイズ 4 段階）・#33（表記ずれ 10 件）はすべて PR #35/#36/#37 で対応済みを実測確認。
- **現在の状態**: M3 は `/reading`・公開ページの静的シェル事前描画と ETag キャッシュ・公開ウィザード UI、**バックエンド骨格（Rust + Axum、新規 crate `server/`）・GitHub アダプター最小実装・公開フロー本体（multipart 取込・novnow 軽量検証・保管単位作成・hub 登録・公開ページ生成）**を実装済み。バックエンドは `GET /meta`・GitHub OAuth + PKCE 認証・セッション管理・認可ミドルウェアを備え、GitHub アクセスはアダプター背後に隠し差し替え可能。公開フローは `POST /projects`（`require_auth` 経由）で zip 受領→検証→保管単位作成→登録→公開 URL 生成まで一括で処理し、失敗時は中間状態を残さない。M4 は Proposal／Issue UI・コメント一覧表示に加え、SQLite-backed ChatPort 参照実装とチャット UI を実装済み。自動マージ・通知センター等は未完了。M5 はセットアップ URL 発行・解決 UI を実装済み。M2 は完了（CI 拡張: 契約スキーマ検証 + Prism モックのスモークテストを docs ジョブに追加）。
- **検証**: `cd web && npm run check && npm run build`、`python3 docs/script/verify-docs.py`、`openapi-spec-validator` による `api/openapi.yaml` 検証を確認済み。Vitest は **40 テスト・16 ファイル**が成功（うちチャット関連は 10 テスト: ChatPort 6・ChatPage UI 2・chat API 2）。astro check は 0 errors、ビルドは **11 ページ**生成（`/read` 重複ルート削除後、実測確認済み）。CI の docs ジョブは `api/openapi.yaml` のスキーマ検証（openapi-spec-validator、OpenAPI 3.1）と Prism モックのスモークテスト（`docs/script/smoke-mock.sh` でモック起動 + 主要 5 パスの 2xx 確認）を実行する。**バックエンド（M3）**: Rust ツールチェーンを `cache/rust-toolchain` ブランチ経由で bootstrap し、`cargo build` が警告 0・エラー 0 で成功。单元テスト **11 件**成功（PKCE・セッション・設定・エラー変換）。実機検証: サーバー起動後 `GET /api/v1/meta` が **200** で契約どおりの応答（`contractVersion`/`serverTime`）、`/auth/github/start`（OAuth 未設定）は **500** で明示エラー、`/auth/refresh`・`/auth/logout`（認証なし）は **401** で契約どおりの Problem 応答。`api/openapi.yaml` は変更なし（契約の正として不変）。

---

## 3. 次セッション開始時の手順

1. コンテナ再起動後は現在のブランチを fetch して追随する:
   ```bash
   git fetch origin arena/e1a89d1a-novnow-hub
   git reset --mixed FETCH_HEAD
   ```
   Rust ツールチェーン復旧が必要な場合は `REF:docs/developers.md` §2.2 を参照する。
2. `REF:docs/script/verify-docs.py` でドキュメント規則を確認する。
3. PR のレビュー・CI 状態を確認する。PR はユーザーの明示指示があるまでマージしない。
4. 次の実装範囲はユーザーの指示を待つ。M4 の残項目は Proposal／Issue／Automerge 等の adapter と契約テスト、自動マージ、通知センター／ファンアウト、Webhook。
5. `REF:api/openapi.yaml` を変更する場合は追加のみとし、OpenAPI validator と生成 API 型チェックを通す。API 仕様書／設計書の対応箇所も同期する。

---

## 4. M3 実装判断（thin slice での方針）

M3 の公開フロー実装において、仕様・設計に明記が無く判断が必要だった点を次に定める。いずれも契約（`api/openapi.yaml`）は変更しない。

1. **公開ページ URL（`publishedUrl`）の形式**。設計 §6.2（`docs/designs/api.md`）は `https://<hub-domain>/{handle}/{slug}` を想定するが、M3 thin slice で既存の web ルーティングはクエリパラメータベース（`/reading/?projectId=…`、`/projects/?projectId=…`、`REF:web/src/lib/links.ts`）である。公開フローが「第三者が URL を開いて本文を読める」ことを満たすため、`publishedUrl` は `{HUB_PUBLIC_BASE}/reading/?projectId={projectId}` として機械生成する。`{handle}/{slug}` のクリーン URL はクリーン URL ルーティングが導入された後続マイルストーンで追加する。`HUB_PUBLIC_BASE` は環境変数で設定し、既定はローカル開発用の `http://localhost:4321`。
2. **hub 登録（プロジェクトメタ）の保持**。設計 §6.2（`projects` テーブル・SQL マイグレーション）は M3 thin slice では未導入（`server/` crate に DB 依存が無い）。M3 ではインメモリのプロジェクト登録簿（`ProjectRegistry`、`Arc<RwLock<HashMap<projectId, ProjectRecord>>>`）でメタを保持し、読み取り（`GET /projects/{projectId}`・`/reading`）がポート経由で保管単位からコンテンツを読む。後続で DB 導入時に登録簿の実装を差し替えのみで移行できる（ポート経由で隔離）。zip は検証後に破棄し、恒久保存しない（仕様 §6.1）。
3. **セッションと利用者の紐付け**。公開フローでレスポンスの `ownerHandle` と保管単位名（`{handle}/{repo}`）が必要。既存の `Session` は利用者 ID のみ保持していたため、`Session` に `Identity`（ID・ハンドル・表示名）を保持させ、`POST /auth/github/callback` で交換済みの `Identity` を渡す。`require_auth` ミドルウェアが返すセッションからハンドルを取り出せる。
4. **multipart と zip 展開の自己完実装**。サンドボックスはオフラインビルドで、vendor に multipart（`multer`）／zip（`zip`／`flate2`）系の crate が未包含であり、crates.io へ到達できない。M3 では (a) multipart/form-data のパース（`Content-Type` の boundary ベース）と (b) ZIP の展開（手書きの DEFLATE 展開＋中央ディレクトリ／ローカルヘッダ解析）を純粋 Rust で自己実装し、外部 crate に依存しない。許可リスト（設計 §10.3）は展開時にストリームで検査する。
5. **novnow 軽量検証の具体的仕様**。設計 §2.1・§5.11 は「必須要素・文字コード・マニフェストの整合性」および「必須パス・UTF-8・`schema_version` を検査」と定めるが、具体的な必須パスは未定義だった。ユーザー判断により、軽量検証の判定は **novnow 本体の仕様（novnow リポジトリの `specifications/project.md` §4.2・§4.3）と同一** とし、hub 独自の必須要素を発明しない。必須ファイルは `project.toml`・`characters.md`・`words.md`・`materials/index.json`・`plots/all.cnov`、必須ディレクトリは `contents/`（空でも可）・`plots/`。`project.toml` は TOML として読め、`schema_version` が `2` または省略は OK、`2` より大きい版は拒否、TOML として読めない場合は拒否する。文字コードは文書ファイルの UTF-8 妥当性を検証し、不正バイトがあれば該当ファイルを列挙して拒否する。任意パス（`ideas.md`・`prompts/`・`agents/`・`exports/`・`.novnow/`）の欠落・未知ファイル・README は拒否理由にせず、不足ファイルを hub 側で自動生成して補うこともしない。本文 `.cnov` の構文診断までは軽量検証では行わない（M4 以降の novnow 診断へ委ねる）。いずれか失敗時は保管単位を作らず `400` で欠落・不正ファイルの一覧を返し、zip は検証後に破棄する（仕様 §6.1）。本判断は後続（M4 以降の本格化）で novnow 固有の検証ルールへ差し替え可能にする。
6. **PR #42 修正指示の判断（zip ディレクトリ entry・バイナリ・クリーンアップ・複数行 TOML・schema_version 検査範囲）**。PR #42（M3 公開フロー本体）に対する 5 件の修正指示に対応するための判断を定める。(a) **zip ディレクトリ entry の扱い**: 通常の zip（`zip -r` 相当）はディレクトリ entry（末尾 `/`、または Unix モードのディレクトリ）を含む。これをファイル化すると幽霊ファイルが生まれ、保管基盤のコミットで失敗（Git 422 等）する。ディレクトリ entry は展開結果の `files` ではなく `dirs` に分類し、必須ディレクトリ（`contents/`・`plots/`）の存在判定はファイルの有無とディレクトリ entry の存在の双方で行う。空の `contents/` はディレクトリ entry のみでも受理する。`__MACOSX/`・`.DS_Store` などのノイズ entry は展開結果から除外する。(b) **バイナリファイルのエンコーディング**: コミット時に UTF-8 として妥当ならテキスト、そうでない（バイナリ相当）は Base64 でコミットし、内容を壊さない（`from_utf8_lossy` で黙って文字化けさせない）。(c) **保管単位作成後の失敗時クリーンアップ**: 保管単位の作成後、コミット・公開範囲設定のいずれかで失敗した場合、中間状態を残さないために `delete_project_shell` で保管単位を削除する。削除自体が失敗した場合はその旨をエラーに含める。(d) **複数行 TOML 値**: `project.toml` のマルチライン値（配列・三重引用符のマルチライン文字列）を読めるように、`[`・`]` の深さと引用符状態を行間で追跡して平衡を検証する。不平衡なマルチライン配列・閉じていない文字列は TOML として読めず拒否する。(e) **`schema_version` 検査の範囲**: `schema_version` の検査は**トップレベルのキーのみ**とし、テーブル内の `schema_version` は検査対象外（省略扱い）とする。上記は `api/openapi.yaml` の契約を変更しない。検証は `real_zip_with_dir_entries_passes_full_verify` テスト（ディレクトリ entry 含む本物 zip を `import_zip` → `verify_snapshot` まで通す）で実行検証する。
