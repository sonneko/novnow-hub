# 設計書: API 契約（API）

本書は `REF:docs/specifications/api.md` の各節に対応する実装設計を述べる。見出しは仕様書と一致させ、各 `###` 節の冒頭に実装状態を置く。契約の唯一の正は `REF:api/openapi.yaml`（OpenAPI 3.1）であり、本書はその実装方針と検証方法を補足する。

---

## 1. 設計原則

### 1.1 契約先行・実装非依存

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock, REF:api/openapi.yaml）

契約駆動の開発手順を CI に組み込む。(1) `api/openapi.yaml` のスキーマ妥当性を検証（`openapi-spec-validator`）、(2) 契約から TypeScript の型とフェッチャを生成し、未コミットの差分があれば CI を落とす、(3) モックを契約から起動してスモークテストする。バックエンド未着手の間、フロントエンドはモックに対してのみ動作すればよい。

### 1.2 GitHub 概念の非漏洩

状態: 未実装・実装計画済み（REF:api/openapi.yaml, REF:docs/plans.md#m3-thin-slice）

契約レビューのチェックリストに「GitHub 固有語（repository／pull／fork／octokit／installation 等）が `REF:api/openapi.yaml` に現れないか」を含め、機械検査（禁止語リストによる grep チェック）を CI に追加する。OAuth 認可 URL とスコープの記述のみ GitHub を指し得るが、これは認証方式の記述でありリソース語彙ではないので許容する。

### 1.3 後方互換と段階的進化

状態: 未実装・実装計画済み（REF:api/openapi.yaml, REF:docs/plans.md#m7-ops）

契約の変更は semver（`info.version`）で管理し、破壊的変更は `/api/v2` の新設と並行運用で吸収する。`v1` のサンセット手順（90 日告知 → `Deprecation`／`Sunset` ヘッダ付与 → 410 応答）を運用ドキュメント化するのは M7。契約の互換性検査（旧版スキーマを入力として新版で `problem+json` にならないこと、必須化・改名がないこと）を CI の互換性ジョブに入れる。

### 1.4 最小限の API 面

状態: 未実装・実装計画済み（REF:api/openapi.yaml）

エンドポイントの追加は必ずユースケース（仕様書 §5 のどれか）に紐付け、紐付かない提案は受け付けない。フィールド追加は省略可能（`nullable` ではなくキー省略）とし、既定値を契約の `description` に明記する。

---

## 2. 用語

状態: 実装完了（REF:docs/specifications/api.md）

仕様書 §2 の用語を正とし、コード識別子・DB スキーマ・ログのキーも同語彙に統一する（`project`／`proposal`／`issue`／`derivative`／`setup`）。ログに `repo`／`pr` などの略称を混入させないよう、構造化ログのフィールド名をレビュー対象とする。

---

## 3. 認証と認可

<a id="api-auth"></a>
### 3.1 GitHub OAuth によるサインイン

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

認可コード + PKCE フロー。バックエンド（別リポジトリで開発）は `/auth/github/start` で `state`（CSRF 対策、10 分 TTL、使い捨て）と `code_challenge` を発行・短期保存して GitHub の認可 URL へリダイレクトする。`callback` で code を交換し、`IdentityPort.exchangeCodeForIdentity` へ渡す。アクセストークンの交換・保管はアダプター層に閉じ込め、ユースケース層は hub の利用者像だけを受け取る。

### 3.2 セッションとトークン

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

セッションは不透明乱数（256bit）+ サーバ照合（JWT にしない）を既定とし、`sessions` テーブル（id／user_id／refresh_hash／expires_at／revoked_at／created_at）に保持する。アクセストークンは 1 時間、更新トークンは 30 日・ローテーション・旧トークンの再利用検知（再利用時は当該利用者の全セッションを失効）を実装する。Tauri 側は secure storage（Keychain／Credential Manager）、ブラウザ側はメモリ + 必要最小限の永続化とする。

### 3.3 認可モデル

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

認可判定はミドルウェア（Axum の `FromRef` + カスタム extractor）で一箇所に集約し、ハンドラからの直接判定を禁止する。Phase 1 は GitHub 権限を `owner`／`collaborator`／`reader` へマップし、プロジェクトごとの判定結果を 5 分キャッシュする。Phase 2 では `AuthorizationPort`（`resolveRole(userId, projectId)`）へ差し替えられる構造とする。非公開対象への reader アクセスは 404 で応答する位置づけとする（存在秘匿ポリシー）。

### 3.4 サインアウトとトークン失効

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

失効は `sessions.revoked_at` の更新 + インメモリ／KV のセッションキャッシュの無効化で行う。退会はトランザクションでセッション全失効・利用者の匿名化・`IdentityPort.revokeGrant`（GitHub の許可取消し API）を実行する。GitHub 側リポジトリは利用者資産として残置する旨を退会フローの画面で明記する。

---

## 4. 共通仕様

### 4.1 ベース URL とバージョニング

状態: 未実装・実装計画済み（REF:api/openapi.yaml, REF:docs/plans.md#m3-thin-slice）

ベースパス `/api/v1` は Axum のネストしたルータで一元管理し、クライアントは `PUBLIC_API_BASE_URL`（Astro の `PUBLIC_` 環境変数）から注入する。`GET /api/v1/meta` は匿名可で、`{"contractVersion": "1.0.0", "serverTime": "…"}` を返す。ヘルスチェックは外部監視からも叩けるよう認証不要とする。

### 4.2 リクエストとレスポンスの形式

状態: 未実装・実装計画済み（REF:api/openapi.yaml, REF:docs/plans.md#m3-thin-slice）

JSON の読み書きは `serde_json` に統一し、未知フィールドは `#[serde(default)]` + 許容（拒否しない）とする。日時は `time` クレートの RFC 3339 変換、ID は ULID 互換の不透明文字列、列挙値は `serde(rename_all = "snake_case")`。テキスト系ファイルの本文は JSON 文字列、バイナリは `{ "encoding": "base64", "mime": "…", "data": "…" }` エンベロープで返す。

### 4.3 エラーレスポンスの形式

状態: 未実装・実装計画済み（REF:api/openapi.yaml, REF:docs/plans.md#m3-thin-slice）

`Problem` 型（`type`／`title`／`status`／`detail`／`code`／`traceId`）を共通モジュールに置き、全ハンドラのエラー型から `IntoResponse` で変換する。`traceId` は ULID を払い出し、構造化ログ（`tracing`）のスパンに束ねる。`code` 文字列の一覧は契約と `REF:api/openapi.yaml` の `components/schemas/Problem.code` の enum に一致させ、ずれたら CI を落とす。

### 4.4 ページネーション

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

カーソルは `(sortKey, id)` の JSON を Base64URL 化した不透明文字列とし、並び順は更新日時降順 + id 降順の複合ソートで安定化する。`limit` の上限 100 はスキーマで強制する。総件数は返さない（COUNT クエリを発行しない）。フロントエンドは「さらに読み込む」ボタン方式で実装し、無限スクロールは既定にしない。

### 4.5 レートリミット

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m7-ops）

Workers では Rate Limiting バインディング、VPS では `governor` 相当のトークンバケットをミドルウェアで掛ける。認証済みは user_id 単位（例: 600 req/min）、匿名は IP 単位（例: 60 req/min）を既定とし、環境変数で調整可能にする。`RateLimit-*` ヘッダはミドルウェアで一律付与し、超過時は `retryAfterSeconds` を含む `problem+json` を返す。

### 4.6 日時・ID・文字列の表現規則

状態: 未実装・実装計画済み（REF:api/openapi.yaml）

slug 生成はタイトルを NFKC 正規化→小文字化→連続記号の圧縮→先頭末尾のハイフン除去→衝突時 `-2…-99` の順で行う。日本語タイトルのみの場合はローマ字変換せずハッシュ短縮（`p-xxxxxxx`）にフォールバックする。比較・検索索引は NFKC + casefold 済み文字列を別カラムに持つ。

### 4.7 冪等性と楽観ロック

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

書き込み系は `Idempotency-Key` を `idempotency_keys` テーブル（key／user_id／request_hash／response_snapshot／expires_at）に保存し、24 時間は同一応答を返す。`If-Match` はプロジェクト本体が保持する `etag`（更新のたびに版を進めた値）と照合し、不一致は `conflict`（409）。ファイル更新は GitHub の blob SHA をファイル単位の ETag として用い、ContentPort の `getEtag` 経由でのみ参照する。

---

## 5. リソースとエンドポイント

### 5.1 認証（auth）

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:api/openapi.yaml）

`start`／`callback` はリダイレクトを返す GET、`refresh`／`logout`／`account` は JSON POST/DELETE。`callback` は成功時にクライアントのコールバック URL（SPA ルート）へ `#session=…` ではなく一時コードを付けて返し、SPA が直後に `refresh` でセッションを得る形にすると、トークンを URL 履歴に残さずに済む。

### 5.2 ユーザー（users）

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:api/openapi.yaml）

`users/me` はセッションから利用者を引いて返す。`handle` は一意索引を持ち、初回は GitHub の login を正規化（小文字）して採用、変更は許可（`profile_handle_history` を保持して旧 handle のリンクを 301 で案内）。`users/{handle}` の公開像は bio とアイコン・公開作品数のみを返し、所属情報は返さない。

### 5.3 プロジェクト（projects）

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:api/openapi.yaml）

作成系は zip 取込（`multipart/form-data`、上限 50MB）と `archive_url` 指定（`{"source":{"kind":"archive_url","url":"…"}}`、hub がその URL から zip を取得）の 2 経路とする。外部ソースの指定に保管基盤の種別を含めないことで、契約から GitHub の語彙を排除する。取込時は展開→検証→保管単位作成→原子コミット→DB 登録の順で、途中失敗は作成済み保管単位の削除補償を行う。`GET /projects` は表示順 `updated_at DESC, id DESC` でカーソルページネーション。`DELETE /projects/{id}` は hub 側の `visibility='archived'` への論理削除で、GitHub のリポジトリは消さない。

### 5.4 ファイル内容（contents）

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:api/openapi.yaml）

`tree` はパス一覧 + 種別 + サイズ + blob 単位の ETag を返す（1 万ファイルを超える場合はトランケートし `truncated: true`）。`file` はテキスト（UTF-8 妥当）なら `content` 文字列、不正バイトを含む場合は `encoding: "base64"` へ自動切替。`commitFiles` は GitHub の Git Data API（blob → tree → commit → ref 更新）で複数ファイルを原子的にコミットし、ref の先端が読み取り時から動いていた場合は再取得して 1 回だけリトライ、それ以上は `conflict`。

### 5.5 変更提案（proposals）

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration, REF:api/openapi.yaml）

ProposalPort 越しに GitHub PR を操作する。`openProposal` は head/base の分岐名を受け、GitHub の PR を作成して、番号・差分統計・初期チェック一覧を返す。`diff` は GitHub の diff を取得し、novnow 診断差分（M5 以降）をマージして返す。承認は PR review（APPROVE／DISMISS）へ写像し、`requiredApprovals` は GitHub の branch protection と hub の予約条件の小さくない方（厳しい方）を採用する。

### 5.6 自動マージ（automerge）

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration, REF:api/openapi.yaml）

`PUT .../automerge` は GitHub の `enablePullRequestAutoMerge`（GraphQL）を第一選択、無効な環境では hub の cron による定期評価（5 分間隔）へ自動フォールバックする。予約レコードは `automerge_reservations` テーブルに保持し、評価は「必要承認数到達 ∧ 必須チェック緑 ∧ 競合なし ∧ 作者以外の承認」の論理積で行う。発火・失敗・取消しの各イベントを通知種別 `proposal_*` として発行する。

### 5.7 課題（issues）

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration, REF:api/openapi.yaml）

IssuePort 越しに GitHub Issues を操作する。hub 側は一覧の投影（number／title／state／labels／assignee／updated_at）のみをキャッシュし、正は都度 GitHub から取得する。本文中の `Closes #N` を解析して提案との紐付けを自動設定する。テンプレートは `.github/ISSUE_TEMPLATE` 相当の機構を hub 設定として持ち、hub UI から選択させる。

### 5.8 チャット（chat）

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration, REF:api/openapi.yaml）

GitHub ではなく hub 管理ストア（D1／SQLite）に実装する。テーブルは `chat_channels` と `chat_messages`。メンションは `@handle` をパースして `mentions` に保存し、ドメインイベント `MessagePosted` から通知へファンアウトする。編集履歴は `chat_message_revisions` に保持し、削除は論理削除（本文を `［削除されました］` に置換してフラグのみ保持）とする。リアルタイム配信は SSE の導入を M7 で検討し、Phase 1 は 5 秒ポーリングで十分とする。

### 5.9 検索（search）

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:api/openapi.yaml）

`search_index` テーブルへの SQL 検索。正規化は NFKC + casefold + かなの区別なし畳み込み（任意）。タグは複数指定時 AND。ソートは `relevance`（前方一致優先→更新日）と `updated` の 2 種。インデックス更新はプロジェクトの作成・更新・可視性変更の各トランザクションのコミット後に非同期投入する。

### 5.10 通知（notifications）

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration, REF:api/openapi.yaml）

通知は `notifications` テーブルに永続化し、`X-Unread-Count` ヘッダをレスポンスに常時付与する。`POST /notifications/read` は `{"ids": [...]}` または `{"all": true}` を受ける。種別ごとのチャネル設定は `notification_settings`（user_id／kind／channel／enabled）で管理する。配信ワーカー（ファンアウト）はプロセス内または cron でよく、外部 MQ は導入しない（コスト制約）。

### 5.11 novnow 特化機能（novnow）

状態: 未実装・実装計画済み（REF:docs/plans.md#m5-novnow-features, REF:api/openapi.yaml）

`verify` は zip 相当のスナップショットに対して必須パス・UTF-8・`schema_version` を検査し、`verification` レコードとして保存する。`diagnostics`・`exports` は非同期ジョブ（`jobs` テーブル）として受け、`JobPort.enqueue` へ委譲する。`reading` は本文を注釈除去済みの読者表現（HTML 片または構造化ブロック配列）で返し、CDN キャッシュのキーにプロジェクトの ETag を含める。M3 時点では `reading` のみ同期実装し、残りは M5 で投入する。

### 5.12 セットアップ（setup）

状態: 未実装・実装計画済み（REF:docs/plans.md#m5-novnow-features, REF:api/openapi.yaml）

`GET /projects/{id}/setup` は `{ projectId, slug, snapshotVersion, acquisition: { kind: "codeload" | "git", url, ref }, novnow: { minVersion }, displayHints }` を返す。`GET /setup/resolve?u=…` は公開ページのセットアップ URL を解釈し、同じペイロードを返す（アプリが公開 URL から出発できるようにするため）。`acquisition.url` は認証不要の公開 tarball（codeload 相当）を既定とし、私設の場合は短期失効トークン付き URL を発行する。いずれの場合も hub のセッションをアプリへ渡さない。

---

## 6. データモデル

### 6.1 エンティティ一覧と関係

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:api/openapi.yaml）

ER 図は設計文書で管理し、DB マイグレーション（`migrations/NNNN_*.sql`）を正とする。主キーはすべて hub 発行の不透明 ID（ULID）。外部キーは DB 制約を張るが、GitHub 側の ID は hub の主キーに使わず `github_*` 補助カラムに隔離する（差し替え可能性の担保）。派生の連鎖は `derivations` テーブルで保持し、深さは派生作成時に計算して格納する。

### 6.2 プロジェクト（Project）

状態: 未実装・実装計画済み（REF:api/openapi.yaml）

`projects` テーブル。`etag` は `version` 整数を Base64 化した値で、更新系の `If-Match` とファイル ETag とは独立。`visibility` は `public`／`private`／`archived`（取り下げ）の 3 値。`derivative_policy` はライセンスから導出しつつ作者の上書きを許す。`published_url` は `https://<hub-domain>/{handle}/{slug}`。

### 6.3 ユーザー（User）

状態: 未実装・実装計画済み（REF:api/openapi.yaml）

`users` テーブル。`external_identity`（provider／subject／login）は別テーブルに分離し、将来の認証プロバイダ追加に備える。`handle` は一意索引 + 正規化済み検索索引。退会時は `deleted_at` を立てて handle を `deleted-<random>` に書き換え、紐づく作品は owner 名を「退会した利用者」として表示する。

### 6.4 変更提案（Proposal）

状態: 未実装・実装計画済み（REF:api/openapi.yaml）

`proposals` は hub 側の投影キャッシュ（number／state／title／author／updated_at／checks JSON／approval_count）を持ち、差分本体・会話は都度 GitHub から取得する。キャッシュの更新は読み取り時の都度検証 + 書き込み時の即時再取得で行う（Webhook は持ってもよいが必須としない）。`state` の写像は open／approved（承認済みだが未マージ）／merged／closed の 4 値で固定する。

### 6.5 課題（Issue）

状態: 未実装・実装計画済み（REF:api/openapi.yaml）

`issues` 投影キャッシュを `proposals` と同様に管理する。`linked_proposal_number` は本文解析で設定する。ラベルの色・説明は GitHub 側を正とし、hub 側には文字列の配列のみ保持する。

### 6.6 チャットメッセージ（ChatMessage）

状態: 未実装・実装計画済み（REF:api/openapi.yaml）

`chat_messages`（id／channel_id／author_id／body／mentions／created_at／edited_at／deleted_at）。本文は 4000 字上限・UTF-8・NFC。検索・全文索引用の索引は持たない。既読位置は `chat_read_positions`（user_id／channel_id／last_message_id）に保持する。

### 6.7 通知（Notification）

状態: 未実装・実装計画済み（REF:api/openapi.yaml）

`notifications`（id／user_id／kind／project_id／proposal_number／payload JSON／read_at／created_at）。種別は §5.10 と同じ enum とし、`payload` は種別ごとの表示文材料のみ（本文全文は入れない＝再配信しない）。90 日で自動パージする cron を M7 で入れる。

---

## 7. GitHub アダプターのポート定義

<a id="api-ports"></a>
### 7.1 ポートとアダプターの原則

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

バックエンド（Rust）では `ports` モジュールにトレイトを定義し、`adapters/github/` に GitHub 実装、`adapters/store/` に hub 管理ストア実装（チャット・通知・セッション・インデックス）を置く。依存方向は `api → usecase → ports ← adapters`。GitHub SDK（octocrab 等）の型は `adapters/github/` 配下に閉じ込め、`pub` に出さない。アーキテクチャテスト（import 境界の AST 走査）で越境を CI で検出する。

### 7.2 ポート一覧と要求仕様

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m4-collaboration）

仕様書 §7.2 の表に対応するトレイト（`IdentityPort`／`ProjectStoragePort`／`ContentPort`／`DerivationPort`／`ProposalPort`／`AutomergePort`／`IssuePort`／`CheckStatusPort`／`ChatPort`／`SearchPort`）を定義する。メソッドの入出力型はすべて hub のドメイン型で記述し、`Result<T, PortError>` を返す。`PortError` は仕様書 §7.3 の 7 値の enum とする。モック実装（`adapters/mock/`）も同じトレイトで実装し、バックエンドの契約テストと統合テストは必ずトレイト越しに実行する。

### 7.3 共通規約（エラー変換・冪等性・タイムアウト）

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

GitHub の HTTP エラー（403 rate limit／404／422／409／5xx）を `PortError` へ変換する表をアダプター内に一元実装し、ユースケース層の分岐を `PortError` のみに閉じる。429 は `Retry-After` を尊重し、上限 3 回まで待機・再試行。書き込みは `Idempotency-Key` を GitHub の head commit メッセージ末尾マーカー（例: `[idem:<key>]`）または DB の冪等記録で重複検出する。タイムアウトは読み取り 5s／書き込み 30s／派生 120s を `reqwest` の per-request タイムアウトで強制する。

### 7.4 Phase 1 のアダプター構成

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m4-collaboration）

GitHub 実装は `octocrab`（または薄い reqwest ラッパ）で実装し、認証情報は `HUB_TOKEN_ENCRYPTION_KEY` で保護したトークンストアから都度取得する（プロセス環境に平文を置かない）。検索インデックスとチャット・通知は hub 管理ストア実装を既定とする。イベント受信は Phase 1 最小構成では「公開ページ参照時の再検証 + 管理画面の再生成ボタン」とし、Webhook 受信（`POST /webhooks/github`、署名検証必須）は M4 の自動マージ信頼性向上とともに導入する。

### 7.5 差し替え時の保証

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

受け入れは (1) 契約テストをモック／GitHub／自前実装の 3 系統で緑、(2) シナリオテスト（公開→読む→派生→提案→自動マージ→通知）を 3 系統で緑、(3) データ移行検証（GitHub 投影キャッシュを自前ストアへ）をリハーサル、の 3 本柱で行う。Phase 2 着手時にこの節を専用の設計ドキュメントへ展開する。

---

## 8. バージョニングと非推奨

### 8.1 API バージョニング

状態: 実装完了（REF:api/openapi.yaml）

`openapi.yaml` の `info.version` を `1.0.0` で開始し、`GET /api/v1/meta` が同値を返すことを契約テストで固定する。タグ・リリースにも契約版を刻み、変更履歴を `CHANGELOG.md` で管理する（M3 で導入）。

### 8.2 後方互換のルール

状態: 未実装・実装計画済み（REF:docs/plans.md#m7-ops）

CI に「契約互換性ジョブ」を追加し、oasdiff 等で前版との差分を検査する（削除・改名・必須化・型変更を検出したら fail）。互換性判断の例外は RFC 相当の設計レビューを PR に添付する。エラーコードの削除も非互換として検出する。

### 8.3 非推奨の予告と移行期間

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

廃止予告は `Deprecation`（日付）と `Sunset`（日付）ヘッダで行い、フロントエンドは非推奨エンドポイントの使用をビルド警告にする。410 応答の `code` は `gone`（契約に `410` を追加するのは v1 のマイナーリリースで可）とする。

### 8.4 スキーマの進化

状態: 未実装・実装計画済み（REF:api/openapi.yaml, REF:docs/plans.md#m2-frontend-mock）

クライアントは契約生成型をそのまま使い、未知フィールドを参照しない。Zod 等の手動スキーマ検証を二重に書かず、境界（API 応答の入口）でのみ軽量な健全性検査（型は生成物に任せる）を行う。列挙値の追加は常に互換とし、クライアントの switch には既定分岐を必須とする。

---

## 9. エラーコード体系

### 9.1 エラー形式

状態: 未実装・実装計画済み（REF:api/openapi.yaml, REF:docs/plans.md#m3-thin-slice）

`Problem` スキーマを契約と実装で一致させる。`type` の URI は `https://novnow.hub/problems/<code>` を既定とし、専用ページは持たない（`code` が正で、クライアントは `code` で分岐する）。`detail` は利用者向けの日本語文面を既定とし、`Accept-Language` で英語に切替えられる構造とする。

### 9.2 エラーコード一覧

状態: 未実装・実装計画済み（REF:api/openapi.yaml, REF:docs/plans.md#m7-ops）

契約の `components/schemas/Problem.code` の enum を唯一の正とし、本節と `REF:docs/specifications/api.md` §9.2 の表は enum と三位一体で更新する。CI で enum と表の突合を行うスクリプト（`tools/check-error-codes.*`）を M7 で導入する。

### 9.3 クライアントでの解釈

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock, REF:docs/plans.md#m3-thin-slice）

API クライアントの共通層（`src/lib/api/`）で `code` を `AuthRequired`／`NotFound`／`Conflict`／`RateLimited`／`UpstreamDown`／`VerificationFailed`／`Unknown` の内部表現へ写像し、画面層は内部表現のみを扱う。`traceId` はエラー画面の「問題を報告」導線のプリセットに埋め込む。

---

## 10. セキュリティ

### 10.1 トークンの取り扱い

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m7-ops）

GitHub のトークンは `token_store` テーブルに AES-256-GCM で暗号化して保存し、鍵は環境変数のみ（リポジトリに置かない）。復号は Identity／GitHub アダプター内だけで行い、クライアントへ返す経路をコードレビューで禁止する（禁止語 lint）。アクセストークンのログ出力は `tracing` のフィルタでトークン様文字列をマスクする。

### 10.2 通信の保護とヘッダ

状態: 未実装・実装計画済み（REF:docs/plans.md#m7-ops）

全オリジンで HSTS・`X-Content-Type-Options`・`Referrer-Policy`・CSP（`default-src 'self'` + 画像 CDN のみ例外）を付与する。CORS は本番・プレビュー・`http://localhost:4321`（Astro dev）のみを許可リスト化する。状態変更系は Bearer 必須とし、cookie を用いない（CSRF トークン不要な構造）。PKCE の `state` は 10 分使い捨てで検証する。

### 10.3 入力検証とファイル処理

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m7-ops）

zip 取込は許可リスト（UTF-8・50MB・2000 ファイル・圧縮率 100 倍・`../` 禁止・絶対パス禁止・symlink 禁止・デバイスファイル禁止）をストリーム展開で検査する。ファイル名は NFC 正規化の上で許可文字（`[\p{L}\p{N}._\/-]`）に限定する。本文表示は常にエスケープ済み HTML とし、`v-html`／`is:raw` を novnow 由来データへ使わない。

### 10.4 秘密情報の管理

状態: 未実装・実装計画済み（REF:docs/plans.md#m7-ops）

シークレットはデプロイ先のシークレットストア（Workers Secrets／Docker secrets／VPS の `.env` のみ）に置き、`.env.example` にはダミ値のみ記載する。鍵ローテーションは年 1 回とインシデント時に実施し、手順を `docs/operations.md`（M7 で作成）に定める。依存の脆弱性は `cargo audit`／`npm audit` を週次 CI で実行し、critical は 7 日以内に修正する。

---

## 11. モックサーバー

### 11.1 目的と位置づけ

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock）

モックは `REF:api/openapi.yaml` の副産物としてのみ存在する。目的は (a) バックエンド未実装期のフロントエンド駆動開発、(b) CI における契約のスモークテスト、(c) デモ・受入シナリオの実行環境、の 3 つに限定し、本番代替にはならない。位置づけは「契約の生きたドキュメント」であり、モックが契約より先に進化することを禁止する。

### 11.2 モックの作り方

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock, REF:api/openapi.yaml）

構成は 2 層。(1) 開発用モックサーバー: Node + `@stoplight/prism-cli`（OpenAPI 3.1 対応）を `npm run mock` で起動し、既定 `:4010` で契約のエンドポイント・バリデーション・example 応答を提供する。example は `openapi.yaml` の `examples` に集約し、別ファイルへ重複させない。(2) テスト用インメモリモック: `msw`（Mock Service Worker）のハンドラを契約スキーマで検証しながらブラウザ／Vitest 上で差し替える。状態を持つシナリオ（サインイン→公開→派生）は Prism の `--dynamic` ではなく、`msw` 側のシードデータ（生成スクリプト出力、手書き禁止）で再現する。

### 11.3 契約テストによる同一性の検証

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock, REF:docs/plans.md#m3-thin-slice）

契約テストはモック／実バックエンドの双方に同一スイートを走らせる。スイートは (a) スキーマ妥当性（`openapi-spec-validator`、CI）、(b) Prism 起動スモーク（各エンドポイント example 応答がスキーマ適合）、(c) 実バックエンド実装後は `schemathesis` によるプロパティベースのファジング（任意入力で 5xx／型違反を出さない）、の 3 段とする。モックと実装の差分が出た場合は契約を先に修正し、モック→実装→テストの順に追随させる。

### 11.4 モックシナリオと運用

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock）

シナリオは仕様書 §11.4 の 8 類型を最低限サポートし、切替はリクエストヘッダ `X-Mock-Scenario`（例: `auth/expired`・`rate-limit`・`conflict`）で行う。CI では `mock-smoke` ジョブを新設し、モック起動 → 主要 6 経路（メタ取得／サインイン遷移／プロジェクト作成／一覧／読者ビュー／提案一覧）の example 応答を検証する。シードとフィクスチャは `mocks/seed/` に生成物としてコミットし、生成元スクリプトと同一 PR で更新する。モックのポート・起動手順は開発者向けドキュメントと README のクイックスタートに明記する。
