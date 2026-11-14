# 仕様書: API 契約（API）

本書は novnow-hub の API 契約を定義する。対象はクライアント（Web／Tauri）と hub API の間の境界であり、リソース・エンドポイント・認証・エラー・バージョニング・外部基盤へのポート定義を含む。本書にコードの情報は含まれない。本契約を機械的に検証可能な OpenAPI 形式の契約ファイルが別途提供され、本書と契約ファイルは同期して保守される。

---

## 1. 設計原則

### 1.1 契約先行・実装非依存

フロントエンドは本契約だけに依存する。バックエンドの実装言語・実行環境・保管先（組込 DB／サーバーレス DB／GitHub 等）が何であれ、クライアントのコードと表示は変わらない。新機能は「契約 → モック → フロントエンド → バックエンド」の順に追加する。

### 1.2 GitHub 概念の非漏洩

本契約と本書には GitHub の製品概念を持ち込まない。利用者向けの語彙は hub 独自のもの（プロジェクト・変更提案・課題・派生・セットアップなど）に統一し、裏側が GitHub であることを識別情報・フィールド名・エラー種別から推測できてはならない。OAuth の認可画面で GitHub が表示されることは、認証方式として GitHub OAuth を選ぶ以上避けられないが、それ以外の経路では GitHub の語彙・応答構造・ID 体系を契約に現さない。

### 1.3 後方互換と段階的進化

契約は「広げて、消さない」を原則とする。フィールドの追加は常に省略可能とし、既存クライアントを壊さない。破壊的変更が不可避の場合は新バージョンの経路を用意し、旧バージョンを一定期間並行運用する。

### 1.4 最小限の API 面

エンドポイントとフィールドは、利用者に見える機能を実現する最小限に留める。便利さのために保管構造をそのまま公開することはしない。これはサーバーコストと実装・検証の容易性を守るための硬い制約である。

---

## 2. 用語

| 用語 | 定義 |
|---|---|
| hub API | 本契約が定義する REST/JSON API の総称。ベースパスは `/api/v1` |
| リソース | URL で指し示される対象一式（プロジェクト、変更提案など） |
| ポート | hub API の内部と外部基盤（GitHub 等）を隔てる境界のインターフェース。hub の語彙のみで記述する |
| アダプター | ポートを特定の外部基盤に対応づける実装。Phase 1 では GitHub 実装を提供する |
| セッション | hub API が発行する、利用者を一意に識別する短期トークンと更新用トークンの一対 |
| 変更提案 | 親プロジェクトへ取り込む変更を提案する単位。差分・会話・状態を持つ |
| 課題 | 作品の問題・要望・議論の単位。状態と会話を持つ |
| 派生 | 既存プロジェクトから自分のコピーを作ること。派生元との関係を hub が保持する |
| pre-setup ビュー | アプリなしでブラウザが本文などの公開内容を読むための表現済みデータ |
| セットアップ URL | hub プロジェクトを novnow アプリが自動セットアップするための URL |
| ページネーションカーソル | 一覧系 API が返す続き位置の opaque な識別子 |
| 楽観ロック | 更新対象が読み取り時点から変わっていないことを条件付きで書き込む仕組み |

---

## 3. 認証と認可

### 3.1 GitHub OAuth によるサインイン

- 認可コードフロー（PKCE 対応）で GitHub OAuth を行う。hub API は認可 URL の発行・コールバックの処理・トークン交換を担い、クライアントは認可コードを保持しない。
- スコープは `read:user` 相当と公開用リポジトリの作成・更新に必要な最小限に限定する。`delete_repo` などの破壊的スコープは要求しない。
- サインインに成功すると、hub の利用者像を新規作成または更新し、セッションを発行する。
- 認可が拒否・失敗した場合は、専用のエラー種別でクライアントへ返す。

### 3.2 セッションとトークン

- セッションはアクセストークン（短期・目安 1 時間）と更新トークン（長期・ローテーション付き）の一対で構成する。
- アクセストークンは Authorization ヘッダの Bearer として送る。ログなどに出力してはならない。
- 更新トークンはローテーションし、使用済みの更新トークンの再利用を検知したら当該利用者の全セッションを失効させる。
- セッションの失効・期限切れには専用のエラー種別を返し、クライアントは再サインインへ誘導する。

### 3.3 認可モデル

- 認可の単位は hub プロジェクトに対する役割（owner／collaborator／reader）と、匿名可の公開リソースである。
- owner は全操作可。collaborator は変更提案・課題・チャット・非破壊の更新が可。reader と匿名利用者は公開リソースの読み取りのみ可。
- Phase 1 では役割は GitHub 側の権限から導出する。判定結果は hub がキャッシュし、変更は短時間で反映される（目安 5 分以内）。
- 権限がない操作には 403 相当の専用エラー種別を返す。対象の存在自体を秘匿すべき場合は 404 相当で応答する（非公開プロジェクトの読み取りなど）。

### 3.4 サインアウトとトークン失効

- サインアウトはセッション単位の失効を行う。
- 退会は、当該利用者の全セッション失効・hub 上の個人情報の削除・OAuth 許可の取消しを行う。作品（GitHub リポジトリ）は利用者の資産として残る。
- 失効済みトークンの提示には専用のエラー種別を返し、以後の利用を拒否する。

---

## 4. 共通仕様

### 4.1 ベース URL とバージョニング

- ベース URL は `/api/v1` とする。クライアントはベース URL を設定から注入し、本番・ステージング・ローカルモックを切り替え可能とする。
- バージョンは URL パスで表す。同一ベース URL で複数バージョンを並行運用できる構造を保つ。
- ヘルスチェックと契約の版情報は `GET /api/v1/meta` で匿名取得できる（契約バージョン・サーバ時刻）。

### 4.2 リクエストとレスポンスの形式

- リクエスト・レスポンスとも `application/json; charset=utf-8` を用いる。
- 未知のフィールドを受け取った場合、リクエストは原則拒否しない（将来の拡張との前方互換のため、読み飛ばす）。ただし契約に定める必須フィールドの欠落・型不一致は 400 相当のエラーとする。
- 日時は RFC 3339 形式の UTC（例 `2026-11-14T09:30:00Z`）、ID は hub 発行の不透明文字列、真偽値は JSON の真偽値、列挙値は小文字スネークケースとする。
- ファイル内容の取得は JSON のエンベロープ内に Base64 で埋め込まず、テキスト系ファイラはプレーンテキスト相当の文字列として返す（novnow の対象は UTF-8 テキスト）。バイナリのアセットは Base64 と MIME 型を返す。

### 4.3 エラーレスポンスの形式

- エラーは RFC 9457（Problem Details）に準じた JSON オブジェクトで返す。

```json
{
  "type": "https://example.com/problems/validation-failed",
  "title": "入力内容に誤りがあります",
  "status": 400,
  "detail": "ライセンスが指定されていません",
  "code": "validation_failed",
  "traceId": "01J…"
}
```

- `code` は機械可読の安定したエラーコードであり、クライアントの分岐はこの値を用いる。`title`・`detail` は人間向け表示用で、将来文言が変わりうる。
- `traceId` はサポート調査のための相関 ID で、ログに対応づく。

### 4.4 ページネーション

- 一覧系はカーソル方式とする。レスポンスは `items` 配列と `nextCursor`（終端では null）を持つ。
- クエリは `cursor`（省略時は先頭）と `limit`（既定 20、上限 100）を受ける。
- 総件数（totalCount）は提供しない（コスト制約）。並び順は各エンドポイントで定義し、既定は更新日時の降順とする。

### 4.5 レートリミット

- 認証済み利用者は利用者単位、匿名は IP 単位でレートリミットを掛ける。
- 制限の単位・残数・リセット時刻を `RateLimit-Limit`・`RateLimit-Remaining`・`RateLimit-Reset` ヘッダで返す。
- 超過時は 429 と `retryAfterSeconds` を返す。クライアントは指数バックオフで再試行する。

### 4.6 日時・ID・文字列の表現規則

- 日時は RFC 3339／UTC。ID は hub が発行する不透明な文字列で、クライアントはその中身を解釈してはならない。
- プロジェクトの正規 URL 識別子（slug）は URL セーフな小文字スラッグとし、重複は `-2` 等の接尾辞で解消する。
- 文字列は NFC 正規化を前提とし、検索・比較は NFKC 相当の正規化を行ってよい。
- 空の配列と未指定は区別する（未指定はキー自体を省略する）。

### 4.7 冪等性と楽観ロック

- 作成系（POST）は `Idempotency-Key` ヘッダを受け付け、同一キー・同一内容の再送では作成を重複させず、前回の結果を返す。
- 更新系（PATCH/PUT）は `If-Match` による楽観ロックを用いる。読み取り時の `etag` を指定して送り、版が進んでいた場合は 409 を返す。
- 削除系は冪等とする（既に削除済みでも 204 相当または 404 を返す。具体は各エンドポイントで定める）。

---

## 5. リソースとエンドポイント

エンドポイント一覧。認証欄の凡例: 「匿名可」= 認証なしで読み取り可、「要認証」= 有効なセッションが必要、「所有者」= プロジェクト所有者のみ。

### 5.1 認証（auth）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/auth/github/start` | 匿名可 | GitHub OAuth の認可 URL を発行する |
| GET | `/auth/github/callback` | 匿名可 | OAuth コールバック。セッションを発行する |
| POST | `/auth/refresh` | 更新トークン | アクセストークンを更新する |
| POST | `/auth/logout` | 要認証 | 当該セッションを失効させる |
| DELETE | `/auth/account` | 要認証 | 退会（個人情報の削除と全セッション失効） |

### 5.2 ユーザー（users）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/users/me` | 要認証 | 自分の利用者像を取得する |
| PATCH | `/users/me` | 要認証 | 表示名・アイコン・通知設定を更新する |
| GET | `/users/{handle}` | 匿名可 | 公開利用者像を取得する |
| GET | `/users/me/projects` | 要認証 | 自分のプロジェクト一覧（非公開含む） |

### 5.3 プロジェクト（projects）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/projects` | 匿名可 | 公開プロジェクト一覧（新着・更新・人気順） |
| POST | `/projects` | 要認証 | 新規公開（zip 取込または既存ソース指定） |
| GET | `/projects/{projectId}` | 匿名可 | プロジェクト詳細（公開ページ用） |
| PATCH | `/projects/{projectId}` | 所有者 | タイトル・説明・タグ・公開範囲・ライセンスの更新 |
| DELETE | `/projects/{projectId}` | 所有者 | 公開の取り下げ（GitHub リポジトリは削除しない） |
| GET | `/projects/{projectId}/derivatives` | 匿名可 | 派生ツリー（子・孫・兄弟） |
| POST | `/projects/{projectId}/derive` | 要認証 | 派生作品を作成する |
| POST | `/projects/{projectId}/releases` | 所有者 | リリース（版）を作成する |

### 5.4 ファイル内容（contents）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/projects/{projectId}/tree` | 匿名可 | ファイルツリー（パス・種別・サイズ・ETag） |
| GET | `/projects/{projectId}/file?path=…` | 匿名可 | ファイル内容の取得（テキストは文字列、バイナリは Base64） |
| PUT | `/projects/{projectId}/file` | 所有者／協働者 | ファイルの作成・更新（楽観ロック） |
| POST | `/projects/{projectId}/commit` | 所有者／協働者 | 複数ファイルの原子的コミット |

### 5.5 変更提案（proposals）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/projects/{projectId}/proposals` | 匿名可 | 提案一覧（状態で絞込） |
| POST | `/projects/{projectId}/proposals` | 要認証 | 提案を作成する（比較元・比較先・差分） |
| GET | `/projects/{projectId}/proposals/{number}` | 匿名可 | 提案詳細（本文・状態・チェック一覧） |
| GET | `/projects/{projectId}/proposals/{number}/diff` | 匿名可 | 差分（novnow 診断差分を含む） |
| POST | `/projects/{projectId}/proposals/{number}/comments` | 要認証 | コメントを追加する |
| POST | `/projects/{projectId}/proposals/{number}/approve` | 要認証 | 承認／承認取消し |
| POST | `/projects/{projectId}/proposals/{number}/merge` | 所有者／協働者 | 即時取り込み |
| POST | `/projects/{projectId}/proposals/{number}/close` | 要認証 | 却下／クローズ |

### 5.6 自動マージ（automerge）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/projects/{projectId}/proposals/{number}/automerge` | 匿名可 | 自動マージの予約状態と条件を取得 |
| PUT | `/projects/{projectId}/proposals/{number}/automerge` | 要認証 | 自動マージを予約する（必要承認数・必須チェック・競合なし） |
| DELETE | `/projects/{projectId}/proposals/{number}/automerge` | 要認証 | 予約を取り消す |

### 5.7 課題（issues）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/projects/{projectId}/issues` | 匿名可 | 課題一覧（状態・ラベルで絞込） |
| POST | `/projects/{projectId}/issues` | 要認証 | 課題を作成する |
| GET | `/projects/{projectId}/issues/{number}` | 匿名可 | 課題詳細（本文・会話） |
| PATCH | `/projects/{projectId}/issues/{number}` | 要認証 | タイトル・本文・ラベル・担当・状態を更新する |
| POST | `/projects/{projectId}/issues/{number}/comments` | 要認証 | コメントを追加する |
| POST | `/projects/{projectId}/issues/{number}/close` | 要認証 | クローズ／再オープン |

### 5.8 チャット（chat）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/projects/{projectId}/chat/channels` | 匿名可 | ルーム一覧 |
| POST | `/projects/{projectId}/chat/channels` | 要認証 | ルームを作成する |
| GET | `/projects/{projectId}/chat/channels/{channelId}/messages` | 匿名可 | メッセージ履歴（カーソル） |
| POST | `/projects/{projectId}/chat/channels/{channelId}/messages` | 要認証 | メッセージを送信する |
| PATCH | `/projects/{projectId}/chat/channels/{channelId}/messages/{messageId}` | 要認証 | メッセージを編集する（投稿者のみ） |
| DELETE | `/projects/{projectId}/chat/channels/{channelId}/messages/{messageId}` | 要認証 | メッセージを削除する（論理削除、投稿者または所有者） |

### 5.9 検索（search）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/search/projects?q=…&license=…&tag=…&sort=…` | 匿名可 | 公開プロジェクト横断検索 |
| GET | `/search/users?q=…` | 匿名可 | 利用者検索 |

### 5.10 通知（notifications）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/notifications` | 要認証 | 自分の通知一覧（カーソル、種別絞込） |
| POST | `/notifications/read` | 要認証 | 指定 ID 一括既読、または全既読 |
| GET | `/notifications/settings` | 要認証 | チャネル・種別ごとの通知設定を取得 |
| PATCH | `/notifications/settings` | 要認証 | 通知設定を更新する |

### 5.11 novnow 特化機能（novnow）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| POST | `/projects/{projectId}/verify` | 所有者／協働者 | novnow 形式の妥当性検証を実行する |
| GET | `/projects/{projectId}/verify/latest` | 匿名可 | 直近の検証結果 |
| POST | `/projects/{projectId}/diagnostics` | 任意 | novnow 診断を実行する（非同期ジョブ） |
| GET | `/jobs/{jobId}` | 要認証 | 非同期ジョブの状態と結果 |
| POST | `/projects/{projectId}/exports` | 匿名可 | 本文のエクスポートを生成する（非同期ジョブ） |
| GET | `/projects/{projectId}/reading` | 匿名可 | pre-setup ビュー（読者向け表現済み本文） |

### 5.12 セットアップ（setup）

| メソッド | パス | 認証 | 概要 |
|---|---|---|---|
| GET | `/setup/resolve?u=…` | 匿名可 | セットアップ URL を解釈し、取得元と手順情報を返す |
| GET | `/projects/{projectId}/setup` | 匿名可 | このプロジェクト用のセットアップ情報を発行する |

---

## 6. データモデル

### 6.1 エンティティ一覧と関係

```
User 1───N Project（owner）
Project 1───N Derivation（派生元→派生先の連鎖）
Project 1───N Proposal 1───N ProposalComment
Project 1───N Issue 1───N IssueComment
Project 1───N ChatChannel 1───N ChatMessage
User 1───N Notification
Project 1───N Release
```

### 6.2 プロジェクト（Project）

| フィールド | 型 | 必須 | 説明 |
|---|---|---|---|
| `id` | string | 必須 | hub 発行の不透明 ID |
| `slug` | string | 必須 | URL セーフな一意識別子 |
| `title` | string | 必須 | 作品タイトル |
| `description` | string | 任意 | 説明（1〜500 字） |
| `ownerHandle` | string | 必須 | 原作者の handle |
| `license` | string | 必須 | `cc0`／`cc-by`／`cc-by-sa`／`cc-by-nc`／`custom` |
| `visibility` | string | 必須 | `public`／`private` |
| `tags` | string[] | 任意 | タグ（最大 10 件） |
| `derivativePolicy` | string | 必須 | `allowed`／`allowed-with-conditions`／`disallowed` |
| `derivativeCount` | number | 必須 | 派生作品数 |
| `defaultBranchName` | string | 必須 | 既定の分岐名（利用者には非表示でもよい） |
| `etag` | string | 必須 | 楽観ロック用の版識別子 |
| `createdAt` / `updatedAt` | datetime | 必須 | 作成・更新日時 |
| `publishedUrl` | string | 必須 | 公開ページの正規 URL |

### 6.3 ユーザー（User）

| フィールド | 型 | 必須 | 説明 |
|---|---|---|---|
| `id` | string | 必須 | hub 発行の不透明 ID |
| `handle` | string | 必須 | hub 上の一意識別子（既定は外部サービスの handle） |
| `displayName` | string | 必須 | 表示名 |
| `avatarUrl` | string | 任意 | アイコン URL |
| `bio` | string | 任意 | 自己紹介（500 字まで） |
| `createdAt` | datetime | 必須 | 利用者像の作成日時 |

### 6.4 変更提案（Proposal）

| フィールド | 型 | 必須 | 説明 |
|---|---|---|---|
| `number` | integer | 必須 | プロジェクト内の通し番号 |
| `title` / `body` | string | 必須 | タイトルと説明 |
| `state` | string | 必須 | `open`／`approved`／`merged`／`closed` |
| `authorHandle` | string | 必須 | 提案者 |
| `baseRef` / `headRef` | string | 必須 | 取り込み先・取り込み元の分岐名 |
| `diffStats` | object | 必須 | 追加・削除行数・変更ファイル数 |
| `checks` | array | 必須 | 必須チェックの一覧（名称・状態・詳細 URL） |
| `approvalCount` / `requiredApprovals` | integer | 必須 | 現在の承認数と必要数 |
| `automerge` | object | 任意 | 自動マージ予約（有効時のみ） |
| `createdAt` / `updatedAt` | datetime | 必須 | 作成・更新日時 |

### 6.5 課題（Issue）

| フィールド | 型 | 必須 | 説明 |
|---|---|---|---|
| `number` | integer | 必須 | プロジェクト内の通し番号 |
| `title` / `body` | string | 必須 | タイトルと本文 |
| `state` | string | 必須 | `open`／`closed` |
| `labels` | string[] | 任意 | ラベル |
| `authorHandle` | string | 必須 | 起票者 |
| `assigneeHandle` | string | 任意 | 担当者 |
| `linkedProposalNumber` | integer | 任意 | この課題を解決する提案 |
| `createdAt` / `updatedAt` | datetime | 必須 | 作成・更新日時 |

### 6.6 チャットメッセージ（ChatMessage）

| フィールド | 型 | 必須 | 説明 |
|---|---|---|---|
| `id` | string | 必須 | hub 発行の不透明 ID |
| `channelId` | string | 必須 | ルーム ID |
| `authorHandle` | string | 必須 | 投稿者 |
| `body` | string | 必須 | 本文（1〜4000 字） |
| `mentions` | string[] | 任意 | 言及された handle |
| `editedAt` | datetime | 任意 | 編集日時 |
| `deleted` | boolean | 必須 | 論理削除か |
| `createdAt` | datetime | 必須 | 投稿日時 |

### 6.7 通知（Notification）

| フィールド | 型 | 必須 | 説明 |
|---|---|---|---|
| `id` | string | 必須 | hub 発行の不透明 ID |
| `kind` | string | 必須 | `derivative_created`／`proposal_merged`／`proposal_comment`／`issue_created`／`mention`／`job_finished`／`publish_rebuilt` など |
| `projectId` / `projectSlug` | string | 任意 | 対象プロジェクト |
| `title` / `body` | string | 必須 | 表示文 |
| `read` | boolean | 必須 | 既読か |
| `createdAt` | datetime | 必須 | 発生日時 |

---

## 7. GitHub アダプターのポート定義

### 7.1 ポートとアダプターの原則

- hub API の内部は「ユースケース層」と「外部基盤への境界（ポート）」に分かれる。ユースケース層はポートの語彙だけを知り、GitHub の SDK・API・ID 体系を一切知らない。
- ポートのメソッド名・入出力は hub の語彙（プロジェクト・変更提案・課題・分岐・コミット）に限定する。`Repository`・`PullRequest`・`Octokit` などの GitHub の語彙はポートより外側に漏らさない。
- GitHub への通信はすべて GitHub アダプター内に閉じ込める。認証ヘッダ・レートリミット処理・応答のパース・エラーの変換もアダプターの責務とする。
- ポートは同期 I/F とする（呼び出しは直列の要求／応答）。非同期が必要な処理は hub 側のジョブとして扱い、ポートには持ち込まない。

### 7.2 ポート一覧と要求仕様

各ポートが提供する操作（hub 語彙）。入出力は hub のデータモデル（§6）に従う。

| ポート | 操作 | 概要 |
|---|---|---|
| IdentityPort | `exchangeCodeForIdentity` | 認可コードを外部サービスの利用者像とアクセス権へ交換する |
| | `revokeGrant` | OAuth 許可を失効させる |
| ProjectStoragePort | `createProjectShell` | 空の保管単位を作成する（リポジトリ相当） |
| | `getProjectMeta` | 公開範囲・既定分岐・ETag を取得する |
| | `setVisibility` | 公開／非公開を切り替える |
| | `deleteProjectShell` | 保管単位を取り下げる（利用可否は基盤の方針に従う） |
| ContentPort | `listTree` | ファイルツリーを取得する |
| | `readFile` | ファイル内容と ETag を取得する |
| | `commitFiles` | 複数ファイルの原子的コミットを作成する |
| | `getEtag` | ファイル／プロジェクトの版識別子を取得する |
| DerivationPort | `deriveProject` | 派生（コピー）を作成し、派生元との対応を返す |
| ProposalPort | `openProposal` | 変更提案を作成する |
| | `getProposal` | 提案の詳細・状態・差分統計を取得する |
| | `listProposals` | 提案一覧（状態絞込） |
| | `diffProposal` | 差分を取得する |
| | `commentProposal` | 会話へコメントする |
| | `reviewProposal` | 承認／承認取消し／却下を記録する |
| | `mergeProposal` | 取り込む |
| AutomergePort | `reserveAutomerge` | 条件付き自動取り込みを予約する |
| | `cancelAutomerge` | 予約を取り消す |
| | `getAutomergeState` | 予約状態と条件を取得する |
| IssuePort | `openIssue` / `getIssue` / `listIssues` / `updateIssue` | 課題の作成・取得・一覧・更新 |
| | `commentIssue` / `closeIssue` | 会話追加とクローズ／再オープン |
| CheckStatusPort | `listChecks` | 必須チェックの一覧と状態を取得する |
| | `registerRequiredCheck` | hub 管理チェックの登録（名称・種別） |
| ChatPort | `listChannels` / `createChannel` | ルームの一覧・作成 |
| | `listMessages` / `postMessage` / `editMessage` / `deleteMessage` | メッセージの取得・送信・編集・削除 |
| SearchPort | `searchProjects` / `searchUsers` | 軽量インデックス検索（hub 側実装を既定とし、外部検索への差し替えも可） |

- Phase 1 で GitHub に第一級のチャット機能は存在しないため、ChatPort は hub 管理ストア（D1／SQLite 等）に実装し、GitHub アダプターは実装しない。Phase 2 でも同一のポートのまま維持する。
- 検索は高コストな全文検索を避け、hub 側の軽量インデックス（タイトル・説明・タグ・作者の前方一致〜部分一致）を既定とする。

### 7.3 共通規約（エラー変換・冪等性・タイムアウト）

- アダプターは外部基盤のエラーを `PortError`（`notFound`／`conflict`／`permissionDenied`／`rateLimited`／`unavailable`／`invalidResponse`／`unknown`）へ一律に変換する。ユースケース層は外部基盤のエラーコードを見ない。
- 書き込み操作は可能な限り冪等キーを付ける。外部基盤が冪等性を保証しない場合は、アダプターが重複作成を検知して結果を収束させる（例: 同一内容の再試行で同じ保管単位を返す）。
- タイムアウトは操作種別ごとに上限を設ける（読み取り 5 秒・書き込み 30 秒・派生作成 120 秒）。タイムアウト時は `unavailable` を返し、リトライ可否は操作の冪等性で判断する。
- 外部基盤のレートリミットはアダプター内で吸収し、待機して再試行できるものは待機する。吸収しきれない場合のみ `rateLimited` を返す。
- 外部基盤の応答 1 件あたりのサイズ上限を超えた場合は `invalidResponse` を返す（異常なトランケートを契約上のデータとして返さない）。

### 7.4 Phase 1 のアダプター構成

- IdentityPort／ProjectStoragePort／ContentPort／DerivationPort／ProposalPort／AutomergePort／IssuePort／CheckStatusPort は GitHub 実装を用いる。
- 認証は GitHub OAuth（Web アプリフロー）を用い、hub は GitHub の access token をサーバ側の暗号化ストアに保持する。クライアントへはセッショントークンのみを返す。
- GitHub の API 呼び出しは hub API のサーバ側からのみ行う。ブラウザ・Tauri アプリは GitHub へ直接通信しない。
- インデックス構築（公開プロジェクトの検索用メタデータ）は公開イベントの受信または定期処理で行う。イベント受信の経路は環境に応じて選定する（例: Webhook 受信、ポーリング）。
- 通知は hub 管理ストアに保存する。GitHub 側の通知設定とは連動させない（利用者の二重管理を防ぐ）。
- チャットは hub 管理ストアに保存する（§7.2）。

### 7.5 差し替え時の保証

- 外部基盤を GitHub から hub 自前の実装へ差し替える場合でも、本契約（§5〜§6）・OpenAPI 契約・エラーコード体系・クライアントの振る舞いは変更しない。
- 差し替えの受け入れ条件は、(1) 本契約に対する契約テストが緑、(2) 主要ユースケースの受け入れシナリオ（公開→読む→派生→提案→自動マージ→通知）が緑、(3) 既存データを読み替えなしで移行できること、の 3 点とする。
- 移行中の整合性のため、二重書き期間（新旧双方へ書いて比突する検証期間）を設けてもよい。期間中も契約は変えない。

---

## 8. バージョニングと非推奨

### 8.1 API バージョニング

- 契約のメジャーバージョンは `/api/v1` のように URL パスで表す。マイナー追加は同一パス内で行う。
- 本契約ファイルには契約の版（例: `1.0.0`）を記載し、`GET /api/v1/meta` で配信する。

### 8.2 後方互換のルール

- 追加は常に後方互換（新しいフィールドは省略可能・列挙値の追加のみ可）。
- 削除・改名・型変更・必須化・意味の変更は破壊的変更であり、次バージョンでのみ許容する。
- エラーコードの削除・意味変更は破壊的変更とする。新しいコードの追加は互換とみなす。

### 8.3 非推奨の予告と移行期間

- 非推奨とするフィールド・エンドポイントは、廃止の 90 日前までに契約とドキュメントで告知し、`Deprecation`・`Sunset` ヘッダで通知する。
- 告知中も現行の動作を保証する。サンセット後は 410 相当の専用エラー種別を返す。

### 8.4 スキーマの進化

- クライアントは未知のフィールド・列挙値を読み飛ばせる実装とする（前方互換）。
- hub API は、利用者が知らない鍵を設定に書き込むことを許し、欠落を補う既定値を明示する。

---

## 9. エラーコード体系

### 9.1 エラー形式

- §4.3 の Problem Details 形式を全エラーで一貫して用いる。HTTP ステータスは 4xx/5xx を適切に使い分け、クライアントの分岐は `code` で行う。

### 9.2 エラーコード一覧

| code | HTTP | 意味 |
|---|---|---|
| `validation_failed` | 400 | 入力値が契約に違反（欠落・型不一致・範囲外・列挙外） |
| `authentication_required` | 401 | 未認証・セッション欠落 |
| `session_expired` | 401 | セッション期限切れ・失効済み |
| `refresh_token_reused` | 401 | 更新トークン再利用検知（全セッション失効） |
| `permission_denied` | 403 | 権限不足（存在は示す） |
| `not_found` | 404 | 対象不存在、または存在を秘匿すべき場合 |
| `conflict` | 409 | 楽観ロック不一致・重複・状態競合 |
| `idempotency_mismatch` | 409 | 同一冪等キーで内容が異なる再送 |
| `payload_too_large` | 413 | 本文・アップロードサイズ超過 |
| `rate_limited` | 429 | レートリミット超過（`retryAfterSeconds` 付き） |
| `upstream_unavailable` | 503 | GitHub 等の外部基盤障害・タイムアウト |
| `verification_failed` | 422 | novnow 形式の検証失敗（公開拒否） |
| `job_not_found` | 404 | 非同期ジョブ不存在 |
| `internal_error` | 500 | 予期しない内部誤り |

### 9.3 クライアントでの解釈

- `authentication_required`・`session_expired` → サインイン画面へ。
- `permission_denied` → 操作不可の理由を表示。`not_found` → 「見つかりません」表示。
- `conflict` → 再読込を促す。`rate_limited` → 再試行時刻を表示して待機。
- `upstream_unavailable` → 読み取り専用への縮退案内。`verification_failed` → 検証詳細を表示。
- 未知の `code` は `internal_error` 相当として扱い、トレース ID を表示する。

---

## 10. セキュリティ

### 10.1 トークンの取り扱い

- hub API は GitHub の access token・refresh token をサーバ側の暗号化ストアにのみ保持する。クライアントへはセッショントークンのみを返し、GitHub のトークンは一切返さない。
- セッションのアクセストークンは短期・ローテーション・再利用検知を備える。保管はメモリと secure storage（Tauri）に限定し、localStorage への恒久保存は避ける。

### 10.2 通信の保護とヘッダ

- 全通信を HTTPS に限定する。HSTS・`X-Content-Type-Options: nosniff`・`Referrer-Policy: strict-origin-when-cross-origin`・`Content-Security-Policy` を付与する。
- CORS はクライアントの正規オリジンのみを許可する。認証付きのクロスオリジン要求は許可しない。
- 状態変更系のリクエストは、Bearer 必須または CSRF トークン併用とし、単純フォーム送信での成立を防ぐ。
- OAuth の `state` と `code_challenge`（PKCE）を必須検証する。

### 10.3 入力検証とファイル処理

- すべての入力をサーバ側で検証する（型・長さ・列挙・正規表現）。URL・パス・ファイル名は許可リスト方式で正規化し、ディレクトリトラバーサル（`../`・絶対パス・区切り混在）を拒否する。
- アップロード zip は展開前にエントリ名・サイズ・圧縮率を検査し、内部リンクのみを許可する。展開サイズ上限・ファイル数上限を超えた場合は拒否し、展開物は検証後に必ず破棄する。
- チャット・コメントの本文は表示時に必ずエスケープまたはサニタイズし、HTML として解釈しない。
- novnow のファイル内容を表示する際も同様とする。

### 10.4 秘密情報の管理

- シークレット（OAuth クライアントシークレット・暗号鍵・DB 接続文字列）は環境変数またはシークレットストアのみに置き、リポジトリ・ログ・エラー応答に含めない。
- 鍵のローテーション手順を運用ドキュメント化する。ログには利用者の本文・トークン・個人情報を出力しない（必要なものはハッシュ化する）。

---

## 11. モックサーバー

### 11.1 目的と位置づけ

- バックエンド実装が存在しない段階からフロントエンドを契約に対して開発・テストできるようにする。
- 契約そのものの妥当性（スキーマの参照整合・例示の妥当性）を機械的に検証する。
- デモ・ドキュメント・受け入れシナリオの実行環境として使う。
- モックは開発用途であり、本番相当のデータを持たない・永続化しない・認証は擬制であって実際の GitHub OAuth を行わない。

### 11.2 モックの作り方

- 正は OpenAPI 契約ファイルとする。モックはこのファイルから起動・生成し、契約とモックの乖離を原理的に排除する。
- 提供形態は 2 つとする。(1) 開発用モックサーバー（契約ファイルを読み込み、エンドポイント・バリデーション・例示応答をそのまま提供するプロセス。ローカルの `npm run mock` 相当で起動し、既定のポートで待受ける）、(2) フロントエンドのテスト用インメモリモック（コンポーネント・画面単位のテストで、ハンドラを契約のスキーマで検証しながら差し替える）。
- モックの応答は、契約に例示（example）として記述したレスポンスを既定とし、手書きの複製をリポジトリに重複させない。例示の作成・更新と契約の変更は同一の変更単位とする。
- 状態を持つシナリオ（サインイン→公開→派生）は、モック内の簡易ストアとシードデータで再現する。シードはフィクスチャ生成スクリプトから生成し、手書き・手動編集を禁止する。
- シナリオの切替は、リクエストヘッダまたは専用のクエリパラメータで行い、クライアントから決定論的に再現できるようにする。

### 11.3 契約テストによる同一性の検証

- 契約テストは「モックに対しても実バックエンドに対しても同一にパスする」ことを目標に設計する。テストは契約のエンドポイントごとに、正常系・境界値・異常系を網羅する。
- CI では契約ファイルのスキーマ妥当性検証とモック起動のスモークテストを実行する。実バックエンドの実装段階では、同じ契約テストを実バックエンドに向けて実行する。
- モックと実装で応答差分が生じた場合は、契約を先に直し、モック・実装・テストの順に反映する。契約が両者の仲裁役である。

### 11.4 モックシナリオと運用

提供するシナリオの最低限:

1. ハッピーパス（サインイン→プロジェクト公開→読む→派生→提案→自動マージ予約→通知）。
2. 認証エラー（未認証・期限切れ・再利用検知）。
3. 入力検証エラー（必須欠落・型不一致・列挙外・巨大ペイロード）。
4. 権限エラー（他者の非公開プロジェクトへのアクセス）。
5. 競合（楽観ロック不一致・冪等キー再送）。
6. レートリミット（429 と再試行時刻）。
7. 上流障害（外部基盤の 5xx・タイムアウト）。
8. 非同期ジョブの遅延（点検・エクスポートが pending を経て succeeded になる）。

- モックの追加・変更は契約の変更と同一の PR で行い、契約外の応答をモックにだけ足さない。
- モックの待受ポート・起動方法は開発者向けドキュメントに明記し、CI とローカルで同一とする。
