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
