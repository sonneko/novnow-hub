# 設計書: 概要（Overview）

本書は `REF:docs/specifications/overview.md` に対応する実装設計を記述する。各節の見出しは仕様書と一致させ、各 `###` 節の冒頭に実装状態を記載する。実装状態の凡例は `REF:docs/developers.md` §1.1 に従う。

---

## 1. 設計原則

### 1.1 非エンジニアでも迷わない公開体験

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m4-collaboration）

実装は「公開ウィザード」（サインイン → 取込 → 確認 → 完了）の 4 ステップに限定し、各ステップの責務を画面コンポーネントに分割する。GitHub 由来の語彙（リポジトリ・コミット・PR）は画面に出さない。内部的には `POST /projects` の 1 トランザクションで GitHub 上の保管単位作成・初期コミット・hub 登録を行う（`REF:docs/designs/api.md` §5.3）。失敗時は `DELETE /projects/{id}` 相当の補償処理で中間状態を除去する。

選択肢の検討: zip アップロードと「手元の GitHub リポジトリを選ぶ」の双方を入り口とする。Phase 1 では zip アップロードのみを既定にし、リポジトリ選択は後続とする。

### 1.2 契約先行とバックエンド非依存

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock, REF:api/openapi.yaml）

`REF:api/openapi.yaml` を契約の唯一の正とし、フロントエンドの API クライアントは契約から型とフェッチャを生成する（`openapi-typescript` + `openapi-fetch` を第一候補）。手書きの API クライアントを置かない。バックエンド実装の有無でフロントエンドのビルド成果物を分岐させない。

### 1.3 最小サーバーコストと OSS

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m7-ops）

デプロイ構成は本命として Cloudflare Workers + D1 + R2 を既定に設計し、代替として VPS + Docker（Axum + SQLite）と Vercel を同一契約のまま選べるようにする。重い処理（エクスポート・点検）は非同期ジョブとし、cron の頻度・同時実行数を環境変数で上限制御する。

### 1.4 GitHub を土台としつつ、いつでも完全に剥がせる

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/designs/api.md#7-github-アダプターのポート定義）

GitHub アクセスは必ずポート（§7 参照）経由に限定し、フロントエンド・契約・ユースケース層から GitHub の語彙を排除する（§1.2 の契約側制約と対になる）。CI や lint で GitHub SDK の import を `adapters/github/**` 配下に限定するアーキテクチャテストを導入する。実装の詳細は `REF:docs/designs/api.md#api-ports` を参照。

### 1.5 データ主権と可搬性

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

作品の正は GitHub リポジトリ側に置き、hub 側には公開ページ表示に必要なメタデータ（タイトル・説明・ライセンス・タグ・slug・派生元）だけを保持する。hub が消えても作品・履歴が失われない構造を、保管単位設計で担保する。退会時は hub 側データのみ削除し、GitHub 側のリポジトリは削除しない（仕様の `DELETE /auth/account` に対応）。

---

## 2. 用語

状態: 実装完了（REF:docs/specifications/overview.md）

仕様書 §2 の用語を正とし、コード上の識別子も hub 語彙（project / proposal / issue / derivative / setup）に統一する。設計上の追加用語は本節に追記し、利用者向け UI 文言は日本語の表示名と英語の識別子を `i18n` 辞書で一対一対応させる。

| UI 表示（ja） | 識別子 | 意味 |
|---|---|---|
| 変更提案 | `proposal` | Pull Request 相当 |
| 課題 | `issue` | Issue 相当 |
| 派生作品 | `derivative` | Fork 相当 |
| 取り込み | `merge` | 提案の取り込み |

---

## 3. 全体像

### 3.1 利用者と役割

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m4-collaboration）

ロールは `owner`／`collaborator`／`reader`（匿名を含む）の 3 種のみ。Phase 1 は GitHub の権限（admin／write／read）からマッピングし、`hub_role` カラムをプロジェクトごとにキャッシュする。キャッシュは書き込み時に失効させ、読み取りは最短でも 5 分以内に反映する TTL とする。

### 3.2 システムの輪郭

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock, REF:docs/plans.md#m3-thin-slice）

```text
apps/web (Astro + Svelte + TypeScript) ─┐
                                        ├─ HTTP/JSON (/api/v1) ─ hub API (Rust + Axum, 別リポジトリまたは別 crate)
src-tauri (Tauri v2) ───────────────────┘                                │
                                                              ┌───────────┴───────────┐
                                                              │ ports (trait)         │
                                                              │ adapters/github/      │
                                                              │ adapters/local-store/ │
                                                              └───────────────────────┘
```

フロントエンドは `PUBLIC_API_BASE_URL` で接続先を切替える（本番／プレビュー／モック）。ブラウザ・Tauri からの GitHub への直接通信は CSP と設計で禁止する。

### 3.3 フェーズ構成

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m7-ops）

Phase 1 は GitHub アダプター群 + hub 管理ストア（チャット・通知・検索インデックス・セッション）で構成する。Phase 2 は `StoragePort` 相当の自前実装（Axum + SQLite／D1）へ差し替える。フェーズ判定はデプロイ設定のみで行い、コード分岐を環境で行わない。差し替え試験は `REF:docs/designs/api.md` §7.5 の受け入れ条件に従う。

---

## 4. アカウントと認証

### 4.1 サインイン（GitHub OAuth）

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/designs/api.md#api-auth）

認可コード + PKCE。`/auth/github/start` で state と PKCE verifier を発行・短期保存し、`callback` で交換する。GitHub の access token はハブ側の暗号化カラム（AES-256-GCM、鍵は環境変数 `HUB_TOKEN_ENCRYPTION_KEY`）に保存し、クライアントへは不透明なセッショントークン（JWT ではなく乱数 + サーバ照合を既定）を返す。セッションは `refresh` ローテーション付き。

### 4.2 権限モデル

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

認可はミドルウェアで強制する（ルートハンドラ内での手判定を禁止）。Phase 1 は GitHub 権限のマッピング（`admin`→owner／`write`→collaborator／`read`・匿名→reader）を、プロジェクト ID ごとにキャッシュして評価する。Phase 2 に向け、権限解決を `AuthorizationPort` として抽出し、Phase 2 では DB 直接引きに差し替えられる形にしておく。

### 4.3 サインアウト・退会

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

サインアウトはセッション ID の論理削除（`sessions.revoked_at`）。退会はトランザクションで (1) 全セッションの失効、(2) 利用者の個人情報の匿名化（handle を `deleted-user` に置換・表示名／アイコン／bio を消去）、(3) OAuth 許可の取消し（GitHub API）、(4) GitHub 上のリポジトリは残置、を行う。監査ログに匿名化済みのイベントのみ残す。

---

## 5. プロジェクトの公開

### 5.1 新規公開フロー

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

UI は 4 画面。API は `POST /projects` に集約し、multipart または JSON（ソース指定）で受ける。サーバ側の処理順は (1) 入力検証 → (2) novnow 検証（§5.3 相当・バックエンド側で再検証）→ (3) 保管単位作成 → (4) 原子コミット → (5) hub 登録と検索インデックス投入 → (6) 公開ページの事前描画。失敗箇所ごとに補償手順を定義し、リトライは冪等キーで吸収する。

### 5.2 公開プロジェクトの実体

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

Phase 1 は 1 hub プロジェクト = 1 GitHub リポジトリ（所有者は原作者の GitHub アカウント）。slug はタイトルから生成し、衝突時は `-2` 以降を付けて一意化する。公開ページは Astro の動的ルートでオリジン `/{handle}/{slug}` に割当て、メタデータは hub の DB、本文は ContentPort 経由で取得し、読者ビュー用に注釈除去済み HTML を短 TTL（60 秒）でキャッシュする。

### 5.3 novnow 形式の検証

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m5-novnow-features）

検証は novnow 本体の診断（`novnow check` 相当）を WASM（novnow rust api を wasm32-wasi 等へビルド）または別プロセスで実行する想定。Phase 1 の M3 では必須ファイルの存在・UTF-8 妥当性・`project.toml` の `schema_version` 上限のみを軽量チェッカーで実装し、完全な診断は M5 で novnow 本体と統合する。結果は検証 API と公開ウィザードの両方に同一の表現で返す。

### 5.4 可視性とライセンス

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

ライセンスは `cc0`／`cc-by`／`cc-by-sa`／`cc-by-nc`／`custom` の 5 値で DB に保存し、`derivativePolicy` はライセンスから機械的に導出する（`cc0`・`cc-by`・`cc-by-sa` → `allowed`、`cc-by-nc`・`custom` → `allowed-with-conditions` または作者指定）。非公開化は GitHub リポジトリの可視性変更 + hub 側の `visibility` 更新の順で行い、失敗時は hub 側をロールバックする。

### 5.5 更新と版

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m5-novnow-features）

更新検知は (a) GitHub からのイベント受信（Webhook 受信エンドポイント）または (b) 公開ページ参照時の短 TTL キャッシュ失効で行う。Phase 1 の最小構成は (b)＋管理画面からの明示的「再生成」ボタンとする。Releases は hub が GitHub の Releases 相当のタグを付与し、リリースノートは hub 側の DB に保持する。

---

## 6. 閲覧と発見

### 6.1 プロジェクトページ

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice）

Astro の動的ルート + Svelte アイランド（インタラクション部分のみ）。公開ページの本文データは `/projects/{id}/reading` で取得し、OGP（タイトル・説明・作者・ライセンス）は SSR でメタタグ化する。設定資料タブは `materials/` 公開フラグをプロジェクト設定として持ち、既定は非公開。

### 6.2 一覧と検索

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m5-novnow-features）

一覧は hub 側の軽量インデックステーブル（`search_index`: title／description／tags／author／updated_at）への SQL 検索。並び替えは新着・更新・派生数の 3 種のみ。検索 API は `/search/projects` に集約し、全文検索は `LIKE` 相当の部分一致 + 正規化（NFKC・小文字化）までとする。ページネーションはカーソル（`updatedAt + id` の複合キーを Base64URL 化）で、総件数は返さない。

### 6.3 ブラウザでそのまま読む

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock, REF:docs/plans.md#m3-thin-slice）

読者ビューは novnow 本文を注釈除去済み HTML として事前生成（公開・再生成時）し、CDN／Workers のキャッシュに乗せる。テーマは `prefers-color-scheme` 追従 + 手動切替、文字サイズは端末内（localStorage 相当）保存。JS 無効でも本文が読めるよう、本文部分は静的 HTML を正とする。

---

## 7. 二次創作支援（novnow 特化）

### 7.1 URL ワンクリックセットアップ

状態: 未実装・実装計画済み（REF:docs/plans.md#m5-novnow-features）

セットアップ URL は `https://<hub>/setup/{projectId}?via=web|desktop` 形式で、`GET /projects/{projectId}/setup` が返すペイロード（取得元・tarball URL／浅い clone URL・検証結果・推奨 novnow 版）を novnow アプリが解釈する。デスクトップ版はカスタムスキーム（`novnow://setup?…`）で受け、ブラウザ版は Web アプリへクエリを渡す。URL に認証情報は含めない。失敗時のフォールバックとして GitHub 上のプロジェクトページへ遷移できる導線を常に表示する。

### 7.2 派生作品の作成（フォーク）

状態: 未実装・実装計画済み（REF:docs/plans.md#m5-novnow-features）

`POST /projects/{projectId}/derive` → DerivationPort.deriveProject。Phase 1 は GitHub Fork API（非同期）を用い、fork 完了をポーリングで確認してから hub に `derivation` レコード（parent_id／child_id／depth）を作成する。ライセンスが派生非許可の場合は 409 を返す。非同期処理のため、派生作成も `202 Accepted + jobId` を返し、完了を通知で知らせる。

### 7.3 派生作品から親作品への還元（変更提案）

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration, REF:docs/plans.md#m5-novnow-features）

派生版→親版の差分を `POST /projects/{parentId}/proposals` で提案化する。差分表示は ContentPort の tree+blob 比較で行い、novnow 診断差分は提案の検証ジョブ（M5）で付与する。承認フローは collaborator 以上が 1 名の承認 + 必須チェック緑で取り込み可（§8.2 と同じ条件式を共有）。

### 7.4 派生ツリーの可視化

状態: 未実装・実装計画済み（REF:docs/plans.md#m5-novnow-features）

`derivations` テーブル（parent_id／child_id／depth／created_at）から再帰 CTE または反復クエリで木を構築し、`/projects/{id}/derivatives` で全深度を JSON 配列で返す。UI は Svelte のツリーコンポーネントで、孫以下は遅延展開とする。原作者通知は `derivative_created` 種別の通知を初回公開時に発行する。

### 7.5 novnow 診断・エクスポート連携

状態: 未実装・実装計画済み（REF:docs/plans.md#m5-novnow-features）

診断・エクスポートは非同期ジョブ（`jobs` テーブル: kind／status／payload／result／created_at）として実行する。Phase 1 のジョブ基盤は Workers の cron トリガまたは GitHub Actions ワークフローを呼び出す形を候補とし、実行環境に依存しないよう `JobPort`（enqueue／status／result）に切り出す。novnow 本体の診断は WASM 化を第一候補、実行不可の場合は GitHub Actions 上の `novnow` CLI を後段に置く。結果 JSON はハブが正規化して保存し、クライアントへ `/jobs/{id}` で返す。

---

## 8. コラボレーション（GitHub 型）

### 8.1 変更提案（プルリクエスト）とレビュー

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration）

ProposalPort を介して GitHub PR を操作する。UI は「提案一覧／詳細／差分／会話」の 4 ビュー。差分は unified diff をサーバで整形し、行単位コメント（Review threads）は GitHub の pull request review comments に写像する。会話の同期は読み取り専用の都度取得 + 書き込み時の即時反映で行い、リアルタイム性は要求しない。

### 8.2 自動マージ

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration）

AutomergePort.reserveAutomerge → GitHub の GraphQL `enablePullRequestAutoMerge`（または hub の cron による定期マージ。GitHub 側の auto-merge が使える設定では前者を既定）。予約条件は DB にも保存し、`requiredApprovals`・`requiredChecks[]`・`requireNoConflicts` を評価する条件式は hub 側で一箇所に実装し、GitHub 側設定と二重管理しない。状態遷移は `reserved → armed → merged | failed | canceled` を許可し、`failed` 時は理由を通知する。

### 8.3 課題（Issue）

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration）

IssuePort を介して GitHub Issues を操作する。ラベルの正は GitHub 側に置き、hub は一覧表示用の投影（id／number／title／state／labels／assignee）だけを保持する。課題→提案の紐付けは本文先頭の `Closes #N` 相当の規約文字列として表現し、hub が解釈して `linkedProposalNumber` を自動設定する。

### 8.4 チャット

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration）

ChatPort は hub 管理ストアに実装する（Phase 1 から GitHub を使わない）。メッセージは `chat_messages` テーブル（id／channel_id／author_id／body／mentions JSON／created_at／edited_at／deleted_at）に保存し、表示はカーソルページネーション。未読は `last_read_message_id` を利用者ごとに保持して算出する。WebSocket は Phase 1 では導入せず、短間隔ポーリング（5 秒）または表示時再取得とする（コスト制約）。

### 8.5 CI（マージ前検証）

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration, REF:docs/plans.md#m5-novnow-features）

CheckStatusPort で GitHub Checks／Commit Statuses を読み、hub 管理チェック（`novnow.verify`）は M5 で GitHub Actions 上の `novnow check` をワークフロー呼び出し（`workflow_dispatch`）または hub ジョブで登録する。必須チェックの登録可否はプロジェクト設定（`require_novnow_check`）で持つ。結果は 5 分間キャッシュし、自動マージ条件評価に用いる。

---

## 9. 通知

### 9.1 通知センター

状態: 未実装・実装計画済み（REF:docs/plans.md#m4-collaboration）

`notifications` テーブル（id／user_id／kind／project_id／payload JSON／read_at／created_at）。発火元はドメインイベント（`DerivativeCreated`／`ProposalMerged`／`JobFinished` 等）で、イベントの発行はユースケース層で行い、永続化とファンアウトを `NotificationService` に集約する。一覧はカーソル、`POST /notifications/read` で一括既読。未読数はヘッダで常時配信（`X-Unread-Count`）し、フロントエンドがバッジに使う。

### 9.2 通知チャネル

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

Phase 1 では hub 内通知のみ。メール・Web Push は M7 以降の検討項目とし、`NotificationChannelPort` の雛形だけを設計段階で確保する。メール送信を入れる場合は OSS の SMTP 連携（Resend／Amazon SES 等）を差し替え可能にし、既定は無効（オプトイン）とする。

---

## 10. 非機能要件

### 10.1 性能目標

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m7-ops）

公開ページは LCP 2 秒を目標に、本文静的化 + CDN キャッシュ + 画像の `loading=lazy` を徹底する。API は読み取りを DB インデックスで p95 100〜200ms に収め、外部往復が支配的な系（一覧・検索）は 300ms を目安に計測する。計測は Workers Analytics／VPS のアクセスログから p50／p95／p99 を週次で確認し、`REF:docs/handoff.md` の記録規約に従い所要時間を記録する。

### 10.2 可用性

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m7-ops）

GitHub 障害時は読み取り縮退（公開ページの静的配信のみ継続・書き込み API は 503 と専用コード）で応答する。稼働監視は外形監視 1 点（UptimeRobot 等の無料枠）+ ヘルスチェック `GET /api/v1/meta`。RPO は GitHub 側に依存（0 とみなす）、RTO は再デプロイ時間（15 分以内）を目標とする。

### 10.3 セキュリティとプライバシー

状態: 未実装・実装計画済み（REF:docs/plans.md#m3-thin-slice, REF:docs/plans.md#m7-ops）

トークン暗号化は §4.1 の通り。zip 取込は許可リスト方式（UTF-8・最大サイズ 50MB・ファイル数 2000・圧縮率 100 倍上限・`../` 禁止・絶対パス禁止・シンボリックリンク禁止）で、展開先は一時ディレクトリのみとし検証後に即削除する。依存の脆弱性は `npm audit`／`cargo audit` を CI に組み込む（M7）。CSP・HSTS・Referrer-Policy はデプロイ層で付与する。

### 10.4 コスト制約

状態: 未実装・実装計画済み（REF:docs/plans.md#m7-ops）

Workers + D1（無料枠）+ R2 を既定とし、無料枠超過の兆候（読み書き回数・ストレージ）を月次で確認する手順を運用ドキュメントに定める。cron は 5 分間隔 1 本 + 日次 1 本の 2 本まで。ジョブの同時実行数は環境変数で 2 に制限する。

### 10.5 アクセシビリティと国際化

状態: 未実装・実装計画済み（REF:docs/plans.md#m2-frontend-mock, REF:docs/plans.md#m7-ops）

Astro + Svelte でセマンティック HTML を基本とし、フォーカス可視・コントラスト 4.5:1・`aria-label` を全インタラクティブ要素に付す。i18n は `astro:i18n` + 辞書 JSON（`ja`／`en`）とし、日時は `Intl.DateTimeFormat` で利用者ロケール表示・内部 UTC 保持とする。

### 10.6 ライセンスとガバナンス

状態: 未実装・実装計画済み（REF:docs/plans.md#m7-ops）

本リポジトリの LICENSE（MIT）を継続し、依存ライセンスの棚卸しをリリース時チェックリストに入れる。仕様・契約の変更は PR 経由 + 人間の承認を必須とする。公開インスタンスの運営ポリシー（削除申立て・権利侵害対応）は M7 で `docs/` 配下に策定する。

---

## 11. 将来の拡張点

### 11.1 Phase 2（hub 自前バックエンド）への移行

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

GitHub アダプターを `adapters/local/`（SQLite + 独自保管）へ差し替える。契約テストの両系統通し + 二重書き検証期間を経て切替える。切替後も API 互換を維持する。

### 11.2 セルフホスト配布物の拡充

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

ワンクリックデプロイ（Cloudflare／Fly.io／自前 Docker）、初期セットアップガイド、GitHub → 自前ストレージの移行ツールを提供する。

### 11.3 作品内検索の高度化

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

SQLite FTS5 または外部検索（Meilisearch 等の OSS）による全文検索と、類似作品の軽量レコメンド（タグ共起ベース）を検討する。

### 11.4 novnow 本体との双方向連携の強化

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

診断設定（project.toml の `[features]`）の hub 上表示・編集、novnow アプリからの直接公開、MCP 連携による hub 操作の許可リスト化。

### 11.5 プラグイン的な拡張

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

独自ライセンステンプレート・独自検証ルール・独自エクスポートを、署名付きマニフェストのプラグインとして読み込む仕組みを検討する。

### 11.6 通知チャネルの追加

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

Web Push（VAPID）・メール（SMTP）・外部チャット（Webhook）を `NotificationChannelPort` の実装として追加する。

### 11.7 出所証明

状態: 未実装・未計画（REF:docs/plans.md#unplanned）

コミット署名・透明性ログ・SLSA プロベナンスにより、公開作品の出所と改変履歴を検証可能にする。
