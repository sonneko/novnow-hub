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
