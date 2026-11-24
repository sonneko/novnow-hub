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

### [HIST:REFACTOR:DOCS-SOURCE-SYNC] 2026-11-21 ドキュメントとソースのリファクタリング（同期修正・整理）
[HIST-TAG: category=REFACTOR id=DOCS-SOURCE-SYNC topics=refactor,docs,source,sync,cleanup]
- コンテキストを断ち切ってドキュメントの主張とソース（`api/openapi.yaml`、`web/`、`scripts/`、`.github/workflows/`）を全件照合し、食い違いを是正した。
- 仕様書 `docs/specifications/api.md`: §5.5／§5.7 にコメント一覧取得 GET（`GET /projects/{projectId}/proposals/{number}/comments`、`GET /projects/{projectId}/issues/{number}/comments`）を追補（契約・実装には既に存在、PR #36 で CommentThread 表示も実装済み）。§6.2 の `Project.visibility` 列挙に `archived` を追加（契約は #31 で 3 値化済み）。§6.6 の `ChatMessage` に `etag`（`If-Match` 用の版識別子）を追補（契約・実装は #23 で対応済み）。
- 設計書 `docs/designs/api.md`: §9.3 の内部表現名を実装（`web/src/lib/api/errors.ts` の kebab-case 表現）と整合させ、状態行を「実装完了」に更新。§6.6 に `etag` の導出方法（`messageVersion`）を追記。§5.8 に実装済みの `If-Match` 照合・5 秒ポーリング・デモ種子を反映。§4.5／§4.7 に契約側で既に定義済みのヘッダ（`RateLimit-*`／`X-Unread-Count`／`Idempotency-Key`）への準拠を明記。§11.4 のシード運用記述を §11.2 の裁定（対象は `mocks/seed` のみ）と整合させた。
- 設計書 `docs/designs/m4.md`: ChatPort メソッド名を実装（`createChannel`／`listChannels`／`postMessage`、#28 改名）と一致させ、`etag` 照合を追記。ProposalPort／IssuePort／AutomergePort のメソッド名を仕様 §7.2 と統一（`openProposal` 等）。§2.1 の状態行を実装範囲（UI・コメント一覧表示は実装済み）に合わせて更新。
- 設計書 `docs/designs/m3.md`: §4.1／§4.2 の状態行を実装範囲（一覧画面・ソート・タグ・ライセンス絞り込みは実装済み、インデックス検索は未実装）に合わせて更新。
- 設計書 `docs/designs/m5.md`: §3.2 の状態行に実装ソース（`SetupResolvePage.svelte` 等）の REF を追補。
- 設計書 `docs/designs/overview.md`: §6.2 の状態行を実装範囲に合わせて更新。§8.4 のチャットテーブル列名を実装（`author_handle`／`deleted` フラグ）と一致させた。
- `docs/developers.md` §7.1: 「テストフィクスチャは手書き禁止」を「モック用シード（`mocks/seed/`）のみ対象、ユニットテストフィクスチャは手書き可」に是正（`docs/designs/api.md#mock-seed` の裁定と整合）。
- `docs/plans.md` M2: 「シード生成スクリプト」を未導入の现状に是正（モックは Prism の example 応答と手書きフィクスチャで代替中）。
- `docs/handoff.md`: テスト数を 40 テスト・16 ファイルに更新（旧「35 テスト」）。ブランチ情報（`arena/a75439b7-novnow-hub`、`main` 最新 `4aeae8d`）とマージ済み PR #34／#35／#36 の記録に更新。
- `README.md`／`web/README.md`: チャット API（`npm run chat:dev`、:4011）の起動・プロキシ経路を明記。`web/README.md` のルート一覧を実装ルートと一致させた。
- ソース: `web/src/pages/read/index.astro`（`/read` ルート）は `/reading` の完全重複で参照元が一切ないため削除（振る舞い・契約は不変。ビルド出力は 12 ページ → 11 ページ）。`api/openapi.yaml` は変更なし（契約の正として不変）。
- 検証: `python3 docs/script/verify-docs.py` 通過。`npm run check`（生成物チェック含む）0 errors、Vitest 40 テスト・16 ファイル成功、`npm run build` 成功。

## 2026-11-21: Issues #22–#33 の解消確認とクローズ

- **ブランチ**: `arena/ae4415dd-novnow-hub`（`main` @ `547e08a` = PR #37 マージコミット）
- **作業**: Issues #22–#33（全 12 Issue）について `main` の現状と照合し、解消確認を実行。全件解消済みと判断し GitHub Issue をクローズ（各 Issue に検証内容のコメントを記録）。
  - **#22**: `api/openapi.yaml` の本文付き 2xx 応答 53 件すべてに example / examples（$ref）が付与されていることを実測確認。
  - **#23**: PUT `/projects/{id}/automerge`・PATCH `/chat/messages/{messageId}`・PATCH `/notifications/settings` の 3 操作に If-Match + 409、notifications/settings は GET/PATCH に ETag を含むことを実測確認（ChatMessage.etag 欄も存在）。
  - **#24**: 作成系 POST 12 操作（/projects、derive、releases、commit、proposals、proposals/{n}/comments、issues、issues/{n}/comments、chat/channels、chat/channels/{id}/messages、diagnostics、exports）すべてが Idempotency-Key を受容することを実測確認。
  - **#25**: `components/headers` に RateLimit-Limit/Remaining/Reset と X-Unread-Count が定義され、59 件の 2xx 応答すべてに RateLimit-Limit が付与されていることを実測確認。
  - **#26**: approve は verdict（approve/dismiss/request_changes）に集約、close は別ライフサイクル操作。ProposalDetailPage.test.ts が 3 経路をテストで固定。
  - **#27**: コメント一覧 GET（proposals/issues）が仕様 `docs/specifications/api.md` §5.5/§5.7 に PR #37 で追補済み。
  - **#28**: ChatPort の操作名（createChannel/listChannels/postMessage 等）が実装（chat-port.d.mts）・テスト・仕様 §7.2 で一致。
  - **#29**: `/meta` の Meta スキーマが contractVersion（pattern）/serverTime/minimumClientVersion を返し、`info.version: 1.0.0` と一致。design §4.1/§8.1 と一致し生成スキーマにも反映。
  - **#30**: POST `/projects/{id}/diagnostics` はグローバル security（bearerAuth 必須）対象で 401 応答も定義。仕様 §5.11 も「要認証」に是正済み。
  - **#31**: `Project.visibility` の enum が public/private/archived の 3 値。i18n ラベル（ja/en）と仕様 §6.2 も 3 値で一致。
  - **#32**: `ReaderPage.svelte` が文字サイズ 4 段階（small/medium/large/xlarge）を実装し、CSS 変数・localStorage（`novnow-reader-font-size`）保存・i18n ラベル（ja/en）完備。
  - **#33**: 表記ずれ 10 件を個別照合し PR #35/#36/#37 で全部是正済み（設計書状態行、plans.md M3、handoff.md 表記、developers.md 参照先、`/read` 重複ルート削除、CI コメント、テストフィクスチャ方針明記、通知 kind 列挙、description minLength、acquisition.kind の codeload→archive 是正）。
- **ドキュメント**: `docs/handoff.md` のブランチ情報（`arena/ae4415dd-novnow-hub`、`main` 分岐点 `547e08a`=PR #37 マージ）と Issues #22–#33 クローズ記録を更新。
- **検証**: `python3 docs/script/verify-docs.py`、`npm run check`、Vitest（40 テスト・16 ファイル）、`npm run build`（11 ページ、実測）を本ブランチで再実行し通過。`api/openapi.yaml` は変更なし（契約の正として不変）。

## 2026-11-21: M2 の CI 拡張（契約スキーマ検証 + モックのスモークテスト）

- **ブランチ**: `arena/34527e8a-novnow-hub`（`main` @ `34689ec` = PR #38 マージコミット）
- **作業**: `docs/plans.md` の M2 未完了項目「CI 拡張: 契約のスキーマ検証とモックのスモークテストを docs ジョブに追加」を完了。
- **現状確認（実行検証）**:
  - スキーマ検証: CI の docs ジョブに既に「Validate OpenAPI contract (api/openapi.yaml)」ステップが存在し、直近の CI 実行（run 44938210687）でステップ結果 `success` を確認（`gh api repos/sonneko/novnow-hub/actions/jobs/...` でステップごとの結果を照合）。ローカルでも `openapi-spec-validator` で `api/openapi.yaml` を OpenAPI **3.1.0** として検証成功。
  - モックのスモークテスト: 既存のワークフローに存在せず、不足していた。
- **変更内容**:
  - `.github/workflows/ci.yml`: docs ジョブに `actions/setup-node@v4`（Node 22、npm キャッシュ = `web/package-lock.json`）と「Smoke test Prism mock」ステップ（`web/` を作業ディレクトリに `bash ../docs/script/smoke-mock.sh`）を追加。timeout を 5 分 → 10 分に拡大。
  - `docs/script/smoke-mock.sh` 新規作成: Prism モックを起動し、読者ビュー・プロジェクトページが接続する主要パス（`/meta`、`/projects/{id}`、`/projects/{id}/reading`、`/projects/{id}/tree`、`/projects/{id}/setup`）へ GET し、すべて 2xx かを検証（plans.md の M2 完了条件「モックに接続した読者ビューとプロジェクトページがローカルで動くこと」と整合）。
  - `docs/plans.md`: 該当チェックボックスを `[x]` に。M2 の状態行を「ほぼ完了」→「完了」に更新。完了条件の達成状況を記録。
  - `docs/handoff.md`: ブランチ情報（`arena/34527e8a-novnow-hub`、分岐点 `34689ec`=PR #38 マージ）・M2 状態・検証行を更新。
- **検証**:
  - ローカル: `bash docs/script/smoke-mock.sh` を実際に実行し 5 パスすべて 2xx（200）を返すことを実測確認。`api/openapi.yaml` の OpenAPI 3.1 スキーマ検証も成功。
  - CI: 本 PR の docs ジョブでスモークテストステップが実際に走ることを確認。初版はステップレベルの `defaults.run.working-directory` がワークフロー検証で失敗したため、`working-directory: web` を直接指定する形に修正。修正後の CI 実行（run 44942112423）で docs ジョブの全ステップ `success`（特に「Smoke test Prism mock」が `completed success`）を確認（`gh api` でステップごとの照合）。
- `web/` と `api/openapi.yaml` の挙動変更は含めていない（CI 定義と検証スクリプト・ドキュメントのみ）。

## 2026-11-21: M3 バックエンド骨格と GitHub アダプター最小実装

- **ブランチ**: `arena/e1a89d1a-novnow-hub`（`main` @ `4f6cb81` = PR #39 マージコミット）
- **作業**: `docs/plans.md` の M3 未完了項目「バックエンド骨格（Rust + Axum、API 契約にのみ準拠）」と「GitHub アダプターの最小実装（保管単位の作成、原子コミット、ツリー／ファイル取得、ETag）」を完了。
- **配置判断**: バックエンドの配置箇所は仕様・設計に未定だったため、新規 crate `server/` に配置（PR 本文に未解決事項として記録）。API 契約の唯一の正は `api/openapi.yaml` で、契約変更は不要（追加も削除も行わない）。
- **変更内容**:
  - `server/` 新規 crate: `Cargo.toml`（axum 0.8 / tokio 1 / reqwest 0.12 rustls / sha2 / base64 / rand / chrono / tracing）、`lib.rs`、`main.rs`（エントリポイント、`/api/v1` ネストルータ）。
  - `ports.rs`: `IdentityPort`／`ProjectStoragePort`／`ContentPort` と `PortError`（7 値）。GitHub SDK 型はアダプター側に閉じ込め、クライアントはポート経由で差し替え可能。
  - `config.rs`: 環境変数からの設定読み込み＋未設定時の明確なエラー（`github_oauth_ready`）。実際の資格情報はコード・ドキュメントに記載なし。
  - `error.rs`: RFC 9457 の Problem 応答、`ProblemCode` 14 値。
  - `session.rs`: 不透明乱数 256bit のセッション、ローテーションと再利用検知。
  - `adapters/github/{mod.rs,client.rs}`: reqwest ラッパ（PKCE S256、トークン交換、Git Data API の blob→tree→commit→ref 原子コミット、429 リトライ 3 回、冪等マーカー）。
  - `routes/{meta.rs,auth.rs}`: `GET /meta` と 5 認証パス。`middleware.rs`: Bearer 認可。
- **検証（実行で証明）**:
  - ビルド: Rust ツールチェーンを `cache/rust-toolchain` ブランチ経由で bootstrap し、`cargo build` が警告 0・エラー 0 で成功。
  - 单元テスト: **11 件**成功（PKCE 決定性・セッション作成/照合/ローテーション/失効・設定既定値・エラー変換）。
  - 実機: サーバー起動後 `GET /api/v1/meta` が **200** で契約どおりの応答（`contractVersion`/`serverTime`）。`/auth/github/start`（OAuth 未設定）は **500** で明示エラー、`/auth/refresh`・`/auth/logout`（認証なし）は **401** で契約どおりの Problem 応答。
- **工具チェーン基盤の整備**: `toolchain-export` ワークフローの Push ステップ連続失敗の真因を特定（`sysroot.tar.gz` が 300MB で GitHub の 100MB ファイル上限を超過 → pre-receive 拒否）。未分割 tar.gz をコミット対象外にし `chunks/` ディレクトリをそのまま追加する形に修正。`bootstrap-rust.sh` の codeload 展開ディレクトリ名を汎用検出に、複数ファイル glob の存在チェックを `compgen -G` に修正。修正後ワークフローは **success** を確認。
- `api/openapi.yaml` は変更なし（契約の正として不変）。

## 2026-11-23: M3 公開フロー（multipart 取込・軽量検証・hub 登録・公開ページ生成）

### [HIST:MILESTONE:M3-PUBLISH-FLOW] 2026-11-23 M3 公開フロー本体を実装（multipart 取込 → 軽量検証 → 保管単位作成 → hub 登録 → 公開ページ生成）
[HIST-TAG: category=MILESTONE id=M3-PUBLISH-FLOW topics=m3,publish,multipart,verify,registry,published-url]
- **ブランチ**: `arena/3f2aec93-novnow-hub`（`main` @ `c279c64` = PR #41 マージコミット）。**マージは指示待ち（未マージ）**。
- **作業**: `docs/plans.md` の M3「公開フロー: zip 取込 → novnow 軽量検証 → 保管単位作成 → hub 登録 → 公開ページ生成」を実装。M3 完了条件「非エンジニアが zip をアップロードして公開でき、第三者が URL を開いて本文を読める」のバックエンド本体。
- **変更内容**:
  - `server/src/multipart.rs`（新規）: multipart/form-data パースを純粋 Rust で自前実装（boundary ベース、外部 crate 依存なし、判断 `docs/handoff.md` §4 判断 4）。テキストフィールドとファイルパートの両方を扱う。
  - `server/src/verify.rs`（新規）: novnow 軽量検証。novnow 本体の仕様（novnow リポジトリの `specifications/project.md` §4.2・§4.3）と同一の判定。必須ファイル（`project.toml`・`characters.md`・`words.md`・`materials/index.json`・`plots/all.cnov`）・必須ディレクトリ（`contents/`・`plots/`）・`project.toml` の TOML 妥当性と `schema_version`（`2` または省略は OK、`2` より大きい版と読めないマニフェストは拒否）・UTF-8 妥当性を検査。不足ファイルの自動生成は行わない。
  - `server/src/registry.rs`（新規）: インメモリ `ProjectRegistry`（判断 `docs/handoff.md` §4 判断 2）。プロジェクトメタを保持し、DB 導入時に差し替え可能。
  - `server/src/routes/projects.rs`（新規）: `POST /projects` ルート。multipart 受領 → zip 取込（許可リスト検査）→ novnow 軽量検証 → 保管単位作成（ポート経由）→ ファイルコミット → 公開範囲設定 → hub 登録 → 公開ページ URL 生成。いずれか失敗時は中間状態（保管単位）を残さずエラー。GitHub 概念は API/ルート層に漏らさずポート経由。
  - `server/src/config.rs`: `HUB_PUBLIC_BASE`（既定 `http://localhost:4321`）を追加。
  - `server/src/session.rs`: `Session` に `Identity` 保持を拡張（判断 `docs/handoff.md` §4 判断 3）。`create_with_identity` 追加、`rotate` で Identity を保持。
  - `server/src/routes/auth.rs`: callback で交換済み Identity をセッションへ紐付け。
  - `server/src/middleware.rs`: `require_auth` がセッションをリクエスト拡張（`RequestIdentity`）へ挿入。
  - `server/src/app_state.rs`・`server/src/main.rs`: `ProjectRegistry` 登録、`POST /projects` ルート追加（`require_auth` 経由）。router 組み立てを lib 側に公開（`build_router`・`build_app_state`）しテストで再利用。
- **検証（実行で証明）**:
  - ビルド: `cargo build --locked --all-targets` → 警告 0・エラー 0 で成功。
  - テスト: `cargo test --locked --all-targets` → **61 件**成功（multipart パース・novnow 検証・registry・公開 URL 生成・router 経由の統合テスト 3 件を含む）。
  - clippy: `cargo clippy --locked --all-targets -- -D warnings` → 通過。
  - 実機: サーバ起動で `GET /api/v1/meta` が **200**、`POST /projects` の認証ゲートが **401**（認証ヘッダーなし・無効トークン）を確認。統合テストで novnow 検証失敗が保管単位作成より前に **400 `verification_failed`** で返り中間状態を残さないこと、有効 zip が検証を通過して保管単位作成へ進むことを検証。
  - ドキュメント: `python3 docs/script/verify-docs.py` → OK（rule-4 見出し構成一致）。
- **ドキュメント同期**: `docs/designs/m3.md` §2.1・§2.2 の状態を「実装完了」に更新、novnow 軽量検証の具体的仕様を追記。`docs/specifications/m3.md` §2.1 に軽量検証の定義を追記。判断記録は `docs/handoff.md` §4 判断 5 に追記。`api/openapi.yaml` は変更なし。

[HIST-TAG: category=PR-FIX id=M3-PUBLISH-FLOW-FIX-PR42 topics=m3,pr42,zip,toml,binary,cleanup]
- **ブランチ**: `arena/3f2aec93-novnow-hub`（PR #42、M3 公開フロー本体）。**マージは指示待ち（未マージ）**。
- **作業**: PR #42 に対する 5 件の修正指示を実装。
- **変更内容**:
  - **修正1（zip ディレクトリ entry）**: `server/src/zip_import/mod.rs` の `import_zip` がディレクトリ entry（末尾 `/`、または Unix モードのディレクトリ）を `files` ではなく `dirs` に分類。ノイズ（`__MACOSX/`・`.DS_Store`）は除外。`server/src/verify.rs` の `verify_snapshot(files, dirs)` が必須ディレクトリをディレクトリ entry も含めて判定し、空の `contents/` を受理。偽名テストを廃止、`real_zip_with_dir_entries_passes_full_verify` を追加（ディレクトリ entry 含む本物 zip を `import_zip` → `verify_snapshot` まで通す）。
  - **修正2（バイナリファイル）**: `server/src/routes/projects.rs` のコミットループを UTF-8 妥当判定で切替。妥当なら `FileEncoding::Text`、そうでなければ `FileEncoding::Base64`（base64 STANDARD エンジン）で内容を壊さずコミット。
  - **修正3（クリーンアップ）**: `server/src/routes/projects.rs` で `commit_files`/`set_visibility` を `commit_result` に包み、失敗時に `delete_project_shell` を呼ぶ。削除失敗時はエラーに含める。
  - **修正4（複数行 TOML）**: `server/src/verify.rs` の `parse_minimal_toml` を `[`・`]` 深さと引用符状態（単一行・三重引用符のマルチライン文字列）を行間で追跡する形に書き換え。複数行配列・三重引用符マルチライン文字列を受理、不平衡は拒否。
  - **修正5（schema_version 検査範囲）**: `schema_version` 検査はトップレベルのキーのみと明記。テーブル内は検査対象外（省略扱い）。
- **検証（実行で証明）**:
  - ビルド: `cargo build --locked --all-targets` → 警告 0・エラー 0 で成功。
  - テスト: `cargo test --locked --all-targets` → **69 件**成功（修正1 の本物 zip 検証を含む）。
  - clippy: `cargo clippy --locked --all-targets -- -D warnings` → 通過。
  - ドキュメント: `python3 docs/script/verify-docs.py` → OK（rule-4 見出し構成一致）。
- **ドキュメント同期**: `docs/designs/m3.md` §2.1 にディレクトリ entry・複数行 TOML・`schema_version` トップレベルのみ・バイナリエンコーディング・クリーンアップの記述を追記。判断記録は `docs/handoff.md` §4 判断 6 に追記。`docs/plans.md` の M3 行を更新。`api/openapi.yaml` は変更なし。
