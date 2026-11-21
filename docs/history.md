# 変更履歴・意思決定アーカイブ（History — 追記専用）

本書は過去の仕様改訂履歴、アーキテクチャ・仕様の意思決定ログ（「最初は X だったが Y に変更した」等の経緯）、および達成済みマイルストーンの記録を保持する**追記専用（append-only）**ファイルである。

---

## 0. `grep` による機械検索ガイド

本書を先頭から全文読まずに目的の経緯を特定するため、すべての記録エントリは次の固定ヘッダとタグ行を持つ。

- 見出し形式: `### [HIST:<CATEGORY>:<ID>] <YYYY-MM-DD> <要約タイトル>`
- タグ行形式: `[HIST-TAG: category=<CATEGORY> id=<ID> topics=<カンマ区切りキーワード>]`
- `<CATEGORY>` の種別:
  - `SPEC-REV`: 仕様書の版改訂履歴
  - `DECISION`: アーキテクチャ・個別設計判断ログ
  - `MILESTONE`: 完了した開発マイルストーンと PR・計測結果
  - `SPEC-FIX`: 仕様の要補正点と裁定アーカイブ
  - `FUTURE`: 将来機能提案の確認アーカイブ

よく使う検索コマンド例:
```bash
# 全エントリの目次と行番号を一覧する
grep -n '^### \[HIST:' docs/history.md

# 特定のカテゴリのみを抽出する
grep -n '\[HIST-TAG: category=DECISION' docs/history.md

# キーワードで検索する
grep -n -i '\[HIST-TAG:.*topics=.*<keyword>' docs/history.md
```

---

## 1. 初期セットアップ

### [HIST:MILESTONE:M0] 2026-10-19 プロジェクト初期セットアップ完了
[HIST-TAG: category=MILESTONE id=M0 topics=setup,template,docs]
- リポジトリテンプレートの作成
- ドキュメント管理規則の策定
- GitHub Actions CI の設定
- 検証スクリプトの作成

## 2. M1 仕様策定の意思決定（2026-11-14）

### [HIST:DECISION:M1-PORT-ADAPTER] 2026-11-14 GitHub 依存はポート・アダプターの背後に隠蔽し、いつでも完全に剥がせる設計とした
[HIST-TAG: category=DECISION id=M1-PORT-ADAPTER topics=github,port,adapter,phase1,phase2,architecture]
- Issue #1 の要件（Phase 1 は GitHub 土台、公開プロジェクト＝GitHub リポジトリ）を採用しつつ、最重要の設計制約として「GitHub 依存を後で完璧に消せること」を M1 で確定させた。
- hub API の内部をユースケース層と外部基盤への境界（ポート）に分け、GitHub へのアクセスは GitHub アダプターに閉じ込める。差し替えの受け入れ条件（契約テスト・シナリオテスト・データ移行リハーサルの 3 点）を仕様と設計に明記した。

### [HIST:DECISION:M1-VOCABULARY] 2026-11-14 hub API の語彙から GitHub 概念を排除した
[HIST-TAG: category=DECISION id=M1-VOCABULARY topics=api,contract,openapi,vocabulary,github]
- 利用者向け語彙を hub 独自の project／proposal／issue／derivative／setup に統一し、repository／pull request／fork 等の GitHub 語彙を契約・データモデル・エラーコードから排除した（認証が GitHub OAuth である点を除く）。
- これにより、GitHub を hub 自前のストレージへ差し替えても API 契約とフロントエンドは不変となる。契約の正は OpenAPI 3.1 の契約ファイルで、版は info.version で管理し /api/v1/meta で配信する。

### [HIST:DECISION:M1-HUB-STORE] 2026-11-14 チャット・通知は GitHub ではなく hub 管理ストアに保存する
[HIST-TAG: category=DECISION id=M1-HUB-STORE topics=chat,notification,github,storage,d1,sqlite]
- Issue #1 の「PR + Issues + Automerge + Chat は必須」に対し、Chat に対する第一級の機能が GitHub に存在しないため、ChatPort を hub 管理ストア（D1／SQLite）に実装する方針とした。
- 通知も hub 管理ストアに保存し、GitHub の通知設定と連動させない（利用者の二重管理を防ぐ）。Phase 1 の公開ページ再生成は最小構成では参照時の再検証＋管理画面の再生成ボタンとし、Webhook 受信は M4 で導入する。

### [HIST:DECISION:M1-MOCK-FIRST] 2026-11-14 薄いスライス優先（M2 モック → M3 公開・閲覧）のマイルストーン順とした
[HIST-TAG: category=DECISION id=M1-MOCK-FIRST topics=milestone,mock,frontend,thin-slice,planning]
- 最初に「URL で開いて読める＋非エンジニアが公開できる」という最小の価値を M3 で届けるため、M2 はフロントエンド基盤とモック接続に限定し、バックエンド未完成の間は契約駆動のモックに接続して開発を進めることとした。
- モックは OpenAPI 契約から起動する開発用サーバーと、フロントエンドテスト用のインメモリモックの 2 層とし、契約を唯一のソースとする。モックの実装は M2 で行う。

### [HIST:DECISION:M1-AUTH-GITHUB] 2026-11-14 認証は GitHub OAuth（認可コード＋PKCE）のみとする
[HIST-TAG: category=DECISION id=M1-AUTH-GITHUB topics=auth,github,oauth,pkce,session]
- hub 独自のパスワード認証は提供せず、GitHub OAuth のみとした。hub は GitHub のトークンをサーバ側の暗号化ストアに保持し、クライアント（ブラウザ／Tauri）へはセッショントークンのみを返す。
- セッションは不透明トークン＋ローテーション付き更新トークンとし、更新トークンの再利用を検知した場合は当該利用者の全セッションを失効させる。

## 3. M2 以降のマイルストーン達成（2026-11-16）

### [HIST:MILESTONE:M3-PUBLISH-WIZARD] 2026-11-16 公開ウィザード UI（PR #8）が main にマージ完了
[HIST-TAG: category=MILESTONE id=M3-PUBLISH-WIZARD topics=m3,publish,wizard,pr8,merge]
- 公開ウィザード UI（非エンジニア向け 4 ステップ）と公開完了画面（URL 提示）を PR #8 で実装し、main へのマージが完了した。
- `docs/plans.md` の M3 項「PR #8 で実装済み、main へのマージ待ち」は古い記述だったため、マージ済みの现状に合わせて更新した。

### [HIST:MILESTONE:M3-READER-VIEW] 2026-11-16 読者ビュー（/reading ルート）の実装が完了
[HIST-TAG: category=MILESTONE id=M3-READER-VIEW topics=m3,reader,reading,ui]
- 読者ビューは `/reading` 配下のルートで提供され、公開ページの URL を開くだけで本文を通読できる。設計書 `docs/designs/m3.md` §3.2 の実装状態を「実装完了」として確定した。
- `docs/plans.md` の M3 項は「読者ビューの本実装（`/reading` + 事前描画とキャッシュ）」を未完了のまま記載していたため、設計書と食い違っていた。読者ビュー（`/reading` ルート）を完了とし、公開ページへの事前描画とキャッシュ（設計書 §3.3）は未実装のまま残した。

### [HIST:MILESTONE:M5-SETUP-URL] 2026-11-16 セットアップ URL 発行・解決（PR #12）が実装・マージ完了
[HIST-TAG: category=MILESTONE id=M5-SETUP-URL topics=m5,setup,url,pr12,merge]
- セットアップ URL の発行（`GET /projects/{id}/setup`）と解決（`GET /setup/resolve`）を PR #12 で実装し、マージ済みである。設計書 `docs/designs/m5.md` §3.1／§3.2 は「実装完了」としていた。
- `docs/plans.md` の M5 項は未完了のままだったため、設計書と食い違っていた。発行と解決を完了とし、novnow アプリ側ハンドラ仕様の追補（設計書 §3.3）は未実装のまま残した。

### [HIST:MILESTONE:M4-COLLAB-UI] 2026-11-16 変更提案 UI と課題 UI（PR #16）が実装・マージ完了
[HIST-TAG: category=MILESTONE id=M4-COLLAB-UI topics=m4,proposal,issue,ui,pr16,merge]
- 変更提案 UI（一覧・詳細・差分・レビュー）と課題 UI を PR #16 で実装し、マージ済みである。
- `docs/plans.md` の M4 項「変更提案 UI（一覧・詳細・差分・レビュー）と課題 UI」を「完了」に更新した。

### [HIST:MILESTONE:M7-SPECS] 2026-11-16 M7 の仕様書・設計書が策定・マージ完了（PR #15）
[HIST-TAG: category=MILESTONE id=M7-SPECS topics=m7,spec,design,docs,pr15,merge]
- M7（運用・品質・OSS 公開準備）の仕様書 `docs/specifications/m7.md` と設計書 `docs/designs/m7.md` を PR #15 で策定し、マージ済みである。
- `docs/plans.md` の M7 節は「仕様・設計策定済み」としており、仕様書・設計書との記述の食い違いや不足はなかった。

### [HIST:SPEC-FIX:M4-COMMENT-LIST] 2026-11-16 M4 契約にコメント一覧取得 GET エンドポイントを追補
[HIST-TAG: category=SPEC-FIX id=M4-COMMENT-LIST topics=m4,openapi,contract,comment,endpoint,additive]
- PR #16 の実装で判明した契約の不足: 変更提案・課題のコメント作成（POST）は OpenAPI 契約に定義があったが、コメント一覧取得（GET）が未定義で、会話スレッド表示が実装できなかった。
- 仕様書 `docs/specifications/m4.md` §2.1・§3.1 と設計書 `docs/designs/m4.md` §2.1・§3.1 にコメント一覧取得の振る舞いを追補し、`api/openapi.yaml` に `GET /projects/{projectId}/proposals/{number}/comments` と `GET /projects/{projectId}/issues/{number}/comments` を追加した。
- コメント一覧はカーソルページネーション（`cursor`・`limit`）に従い時系列順で返し、レスポンスは新規スキーマ `CommentPage` とする。コメント内容は認可を経ずに返さない（仕様書 §8.1）。
- 契約変更は追加のみ（既存 POST の変更・削除なし、既存スキーマの変更なし）であり、破壊的変更はない。

### [HIST:SPEC-FIX:M6-OFFLINE-CACHE] 2026-11-19 M6 設計書の読者ビューオフライン閲覧記述を PR #19 実装に合わせて是正
[HIST-TAG: category=SPEC-FIX id=M6-OFFLINE-CACHE topics=m6,pwa,offline,cache,etag,pr19,docs]
- PR #19（M3 読者ビュー事前描画と ETag キャッシュ）は、読者ビュー本文をプロジェクト識別子と更新 ETag をキーに CacheStorage に保存し、オフライン時にも既読の公開ページ本文を表示できる仕組みを実装済みとした（M3 設計書 §3.3 を「実装完了」に更新）。
- しかし M6 設計書 §3.2 は同一のキャッシュ戦略（キャッシュキー: プロジェクト識別子と更新 ETag）を「未実装・実装計画済み」としており、ソース実装と齟齬があった。
- M6 設計書 §3.2 に「キャッシュ戦略は M3 で先行実装済み（PR #19）であり、M6 は PWA のマニフェスト・オフラインシェルで残余を完成させる」と明記して是正。plans.md の M6 項目に同様の注記を追った。M6 の残り範囲（PWA マニフェスト・オフラインシェル等）は未実装のまま。

### [HIST:SPEC-FIX:ISSUES-22-33-DOCS] 2026-11-20 Issues #22–#33 の同期チェックによるドキュメント側修正
[HIST-TAG: category=SPEC-FIX id=ISSUES-22-33-DOCS topics=docs,sync,spec,design,plans,issues22-33]
- 仕様書 `docs/specifications/api.md`: §4.7 で `Idempotency-Key` の対象となる作成系 POST を 12 操作（`POST /projects`、`/derive`、`/proposals`、`/issues`、`/releases`、`/commit`、提案・課題のコメント POST、`/chat/channels`、チャットメッセージ POST、`/diagnostics`、`/exports`）として列挙し、対象外（auth/refresh、auth/logout、merge、approve、close、notifications/read、verify）を明示した（#24）。§5.5 を契約の判定方式に整合: `approve` は verdict（`approve`／`dismiss`／`request_changes`）を受け取り、`close` はレビュー却下ではなく提案の取下げ・クローズ（ライフサイクル操作）であると明記した（#26）。§5 のエンドポイント表にコメント一覧取得 GET（`GET /projects/{projectId}/proposals/{number}/comments`、`GET /projects/{projectId}/issues/{number}/comments`）が既に記載されていることを確認（契約追加は PR #17 で済み、#27）。§5 エンドポイント表の `POST /projects/{projectId}/diagnostics` 認証を「任意」→「要認証」に是正（計算負荷の大きい非同期 POST の匿名実行の乱用回避のため。公開性の原則は読み取りに適用、#30）。§6.7 の通知 `kind` 列挙を契約の 9 値（`derivative_created`／`proposal_merged`／`proposal_automerge_failed`／`proposal_comment`／`issue_created`／`issue_comment`／`mention`／`job_finished`／`publish_rebuilt`）に整合（#33）。
- 設計書 `docs/designs/api.md`: §4.1・§8.1 を「`GET /api/v1/meta` は `contractVersion` を返し、その値が `openapi.yaml` の `info.version` と等しいことを契約テストで固定する」に統一（#29）。§4 の `acquisition.kind` 値を `codeload` → `archive` に是正（中立語彙への統一。契約・コード側の修正は別セッション、#33）。§11.2 の「シード生成スクリプト、手書き禁止」の対象を `mocks/seed` のモック用シードデータのみと明記し、ユニットテストフィクスチャ（`web/src/test/fixtures.ts`）は対象外・手書き可であることを補記（#33）。
- 設計書 `docs/designs/m5.md` §4: `acquisition.url` の「codeload 相当」→「archive 相当」に是正（#33）。
- 設計書 `docs/designs/overview.md` と `m3.md` §2.1・`m4.md` §2.1・`m6.md` §3.2 の状態行を実装済み範囲に更新: 公開ウィザード UI（PR #8）、読者ビュー・事前描画と ETag キャッシュ（PR #19）、セットアップ URL 発行・解決（PR #12）、変更提案・課題 UI（PR #16）、チャット（PR #21）を反映。残項目（バックエンド本体・公開フロー・検索・自動マージ・通知等）は未実装のまま明示（#33）。
- `docs/plans.md`: M3 概要行に公開ウィザード UI の実装を反映し、項目の完了状態との食い違いを解消（#33）。
- `docs/handoff.md`: 35 テストの内訳を明記（チャット関連は 10 テスト: ChatPort 6・ChatPage UI 2・chat API 2）、ブランチ名 `arena/35aca776-novnow-hub` を現在の `arena/56f1227e-novnow-hub`（main 分岐点 `997f9dd`）に更新（#33）。
- `docs/developers.md`: §2.2 の Rust ツールチェーン復旧パスを `dependencies/rust/bootstrap-rust.sh` に是正。§3 のディレクトリ構成で `mocks/` を「既定パス・未導入（モックは web/ の Prism・msw で代替中）」と実態に合わせて修正（#33）。
- 読者ルートの用語を `/reading` に統一: `web/README.md` の `/read/` を `/reading` に修正し、`docs/designs/m3.md` §3.2 の REF を実体の `web/src/pages/reading/index.astro` に是正（#33）。
- `.github/workflows/ci.yml`: 「スキャフォールド段階では未配置」等の古いコメントを除去。テストは lint ジョブの `npm run check`（Vitest を含む）で実行されている実態に合わせ、`test` ジョブの位置付けを更新（#33）。
- `README.md`: 「試し方」セクションを追加（前提条件・インストール・モック API と開発サーバーの起動手順・試せることの一覧）。手順は実際の起動と各ルートの HTTP 検証（200 応答確認）により検証済み。公開済みデプロイ URL は存在しないためローカル限定の手順とした。
