# 開発計画（Plans）

本書は現在の開発状況サマリ、実装計画、およびマイルストーンを記述する。完了済みの履歴は `REF:docs/history.md` を参照すること。

---

## 1. 全体進捗サマリ

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

## 2. マイルストーン一覧

<a id="m0-setup"></a>
### M0: プロジェクトセットアップ（完了）
- [x] リポジトリの作成
- [x] ドキュメント管理規則の策定
- [x] GitHub Actions の設定
- [x] 検証スクリプトの作成

<a id="m1-specs"></a>
### M1: 仕様書・設計書・API 契約・計画書（完了）
- [x] `docs/specifications/overview.md`（ユーザーから見える振る舞いの仕様）
- [x] `docs/specifications/api.md`（API 契約: リソース・認証・エラー・ポート定義・モック設計）
- [x] `docs/designs/overview.md` と `docs/designs/api.md`（仕様と見出し一致・実装状態付き）
- [x] `api/openapi.yaml`（OpenAPI 3.1、`openapi-spec-validator` 通過、45 パス）
- [x] モックサーバーの設計（契約に対するモックの作り方。実装は M2）
- [x] 本書（M1 の進捗と M2 以降のマイルストーン計画）
- [x] 人間による仕様書・設計書のレビューとマージ（ユーザーが実施）

<a id="m2-frontend-mock"></a>
### M2: フロントエンド基盤 + モック接続
- [x] `web/` の雛形（Astro + Svelte + TypeScript strict、パッケージマネージャは npm）
- [x] 契約からの API クライアント生成（型 + フェッチャ）と CI での生成物差分チェック
- [x] 開発用モックサーバー（OpenAPI から起動）とテスト用インメモリモック（シード生成スクリプトは未導入: モックは Prism の example 応答と `web/src/test/` の手書きフィクスチャで代替中）
- [x] CI 拡張: 契約のスキーマ検証とモックのスモークテストを docs ジョブに追加
- [x] アプリシェル（レイアウト・i18n（ja/en）・ダーク／ライトテーマ・ルーティング）
- [x] 読者ビュー・プロジェクトページの UI 骨格（モックデータ駆動）
- [x] Tauri v2 の雛形（Web と同一コードベース）

完了条件: モックに接続した読者ビューとプロジェクトページがローカルで動くこと。**達成済み**（CI の docs ジョブで Prism モックのスモークテストとして検証: `docs/script/smoke-mock.sh` がモックを起動し、読者ビュー・プロジェクトページが接続する主要パス（/meta、/projects/{id}、/projects/{id}/reading、/projects/{id}/tree、/projects/{id}/setup）が 2xx を返すことを確認）。

<a id="m3-thin-slice"></a>
### M3: 薄いスライス — 公開と閲覧（Phase 1 最小）（仕様・設計策定済み）
- [x] バックエンド骨格（Rust + Axum、API 契約にのみ準拠）: `GET /meta`、認証（GitHub OAuth + PKCE）、セッション、認可ミドルウェア（仕様: REF:docs/specifications/m3.md、設計: REF:docs/designs/m3.md）— 実装済み。配置箇所は仕様・設計に未定だったため新規 crate `server/` に配置（PR 本文に記録）。実機検証: `GET /api/v1/meta` が 200 で契約どおりの応答、認証エンドポイントも契約どおり。单元テスト 11 件成功。
- [x] GitHub アダプターの最小実装（保管単位の作成、原子コミット、ツリー／ファイル取得、ETag）（仕様: REF:docs/specifications/m3.md、設計: REF:docs/designs/m3.md）— 実装済み。GitHub アクセスは `adapters/github/` 配下に閉じ込め、ポート経由で差し替え可能。契約スキーマ・型は `api/openapi.yaml` と `docs/designs/api.md` §7 に準拠。
- [x] 公開フロー: zip 取込 → novnow 軽量検証 → 保管単位作成 → hub 登録 → 公開ページ生成（仕様: REF:docs/specifications/m3.md、設計: REF:docs/designs/m3.md）— 実装済み。multipart/form-data パース（純粋 Rust 自前実装）、`POST /projects` ルート（`require_auth` 経由）、novnow 軽量検証（novnow 本体の仕様 §4.2・§4.3 と同一）、保管単位作成（ポート経由）、インメモリ hub 登録（ProjectRegistry）、公開ページ URL 生成（`{HUB_PUBLIC_BASE}/reading/?projectId=…`）。PR #42 で 5 件修正（zip ディレクトリ entry をファイル化しない・バイナリは Base64 で壊さない・保管単位作成後の失敗時は削除・複数行 TOML 値を読める・`schema_version` 検査はトップレベルのみ）。`cargo build`（警告 0）・`cargo test`（69 件）・`cargo clippy -D warnings` 通過。実機検証: サーバ起動で認証ゲート 401・`/meta` 200 を確認。
- [x] 読者ビュー本体（`/reading` の API 接続・本文表示、設計 §3.2）（仕様: REF:docs/specifications/m3.md、設計: REF:docs/designs/m3.md）
- [x] 公開ページへの事前描画と ETag キャッシュ（設計 §3.3）（仕様: REF:docs/specifications/m3.md、設計: REF:docs/designs/m3.md）
- [ ] プロジェクト一覧・検索の最小実装（軽量インデックス、カーソルページネーション）（仕様: REF:docs/specifications/m3.md、設計: REF:docs/designs/m3.md）
- [x] 公開ウィザード UI（非エンジニア向け 4 ステップ）と公開完了画面（URL 提示）（PR #8 で実装・マージ済み）（仕様: REF:docs/specifications/m3.md、設計: REF:docs/designs/m3.md）

完了条件: 非エンジニアが zip をアップロードして公開でき、第三者が URL を開いて本文を読めること（本番相当環境）。

<a id="m4-collaboration"></a>
### M4: コラボレーション（変更提案・課題・自動マージ・チャット・通知）（仕様・設計策定済み）
- [ ] ProposalPort／IssuePort／AutomergePort／CheckStatusPort の GitHub 実装と契約テスト（仕様: REF:docs/specifications/m4.md、設計: REF:docs/designs/m4.md）
- [x] 変更提案 UI（一覧・詳細・差分・レビュー）と課題 UI（PR #16 で実装済み）（仕様: REF:docs/specifications/m4.md、設計: REF:docs/designs/m4.md）
- [ ] 自動マージの予約・取消・状態表示（GitHub の auto-merge または cron 評価）（仕様: REF:docs/specifications/m4.md、設計: REF:docs/designs/m4.md）
- [x] ChatPort の hub 管理ストア実装とチャット UI（Phase 1 は 5 秒間隔の短間隔ポーリング）（仕様: REF:docs/specifications/m4.md、設計: REF:docs/designs/m4.md）
- [ ] 通知センターと未読バッジ、ドメインイベントからの通知ファンアウト（仕様: REF:docs/specifications/m4.md、設計: REF:docs/designs/m4.md）
- [ ] GitHub Webhook 受信（署名検証）による公開ページ再生成とキャッシュ失効（仕様: REF:docs/specifications/m4.md、設計: REF:docs/designs/m4.md）

完了条件: 派生 → 変更提案 → 承認 → 自動マージ → 通知、の一連の流れが本番環境で確認できること。

<a id="m5-novnow-features"></a>
### M5: novnow 特化機能（セットアップ URL・派生ツリー・診断）（仕様・設計策定済み）
- [ ] DerivationPort（GitHub Fork 連携 + 派生レコード）と派生ツリー UI（`/derivatives`）（仕様: REF:docs/specifications/m5.md、設計: REF:docs/designs/m5.md）
- [x] セットアップ URL 発行と解決（`/projects/{id}/setup`、`/setup/resolve`）（PR #12 で実装済み）（仕様: REF:docs/specifications/m5.md、設計: REF:docs/designs/m5.md）
- [ ] novnow アプリ側ハンドラ仕様の追補（仕様: REF:docs/specifications/m5.md、設計: REF:docs/designs/m5.md）
- [ ] novnow 診断・エクスポートの非同期ジョブ基盤（JobPort）と実行環境（novnow の WASM 実行、不可なら GitHub Actions 上の novnow CLI）（仕様: REF:docs/specifications/m5.md、設計: REF:docs/designs/m5.md）
- [ ] 変更提案差分への novnow 診断差分（新規混入／解消）の付与（仕様: REF:docs/specifications/m5.md、設計: REF:docs/designs/m5.md）
- [ ] 公開前検証の本格化（novnow check 相当の診断を必須チェックに連動）（仕様: REF:docs/specifications/m5.md、設計: REF:docs/designs/m5.md）

完了条件: セットアップ URL 一発で novnow アプリが作品を開き、診断結果が変更提案フローに乗ること。

<a id="m6-multiplatform"></a>
### M6: マルチプラットフォーム（Tauri・PWA）（仕様・設計策定済み）
- [ ] Tauri v2 によるデスクトップビルド（Windows／macOS／Linux）と配布パイプライン（仕様: REF:docs/specifications/m6.md、設計: REF:docs/designs/m6.md）
- [ ] PWA 化（manifest、オフラインシェル、読者ビューのオフライン閲覧）（読者ビュー本文のキャッシュ自体は M3 で先行実装済み、PR #19）（仕様: REF:docs/specifications/m6.md、設計: REF:docs/designs/m6.md）
- [ ] Tauri secure storage へのセッション保管とカスタムスキームでのセットアップ URL 受信（仕様: REF:docs/specifications/m6.md、設計: REF:docs/designs/m6.md）
- [ ] インストーラ署名またはストア配布の方式決定と実装（仕様: REF:docs/specifications/m6.md、設計: REF:docs/designs/m6.md）

完了条件: デスクトップアプリがセットアップ URL 経由で作品を開き、Web と同一の操作体系であること。

<a id="m7-ops"></a>
### M7: 運用・品質・OSS 公開準備（仕様・設計策定済み）
- [ ] 本番デプロイ（Workers + D1 を既定、VPS + Docker を代替）と外形監視・バックアップ・鍵ローテーション手順（仕様: REF:docs/specifications/m7.md、設計: REF:docs/designs/m7.md）
- [ ] CI の拡張（契約互換性検査（oasdiff 相当）、`cargo audit`／`npm audit`、カバレッジゲート）（仕様: REF:docs/specifications/m7.md、設計: REF:docs/designs/m7.md）
- [ ] セキュリティレビュー（zip 取込・CSP・トークン暗号化・権限判定）と依存ライセンスの棚卸し（仕様: REF:docs/specifications/m7.md、設計: REF:docs/designs/m7.md）
- [ ] ドキュメント整備（セルフホストガイド・運営ポリシー・CHANGELOG）と初回リリース（仕様: REF:docs/specifications/m7.md、設計: REF:docs/designs/m7.md）
- [ ] 性能目標の実測検証（LCP・API p95・検索）とコスト実測（仕様: REF:docs/specifications/m7.md、設計: REF:docs/designs/m7.md）

完了条件: OSS として一般公開できる状態。

---

## 3. フェーズ別実行順序

1. **M2 → M3**: 契約とモックを先に固め、薄いスライス（公開と閲覧）を最短で本番に乗せる。M3 完了時点で「URL で開いて読める」という最小の価値が生まれる。
2. **M4 と M5**: M4 は M3 の認証・プロジェクト基盤の上にコラボレーションを載せる。M5 は M3 の検証基盤と M4 の提案フローに乗るため、M4 と並行しつつ順次投入する。
3. **M6 → M7**: マルチプラットフォーム化は Web が安定してから、運用・公開準備は最後に一括で行う。

依存関係の要点:

- バックエンド（別途開発）は API 契約にのみ準拠すればよく、M2 のフロントエンドはモックで先行開発できる。
- GitHub アダプターは M3 の最小 2 ポートから始め、M4 でコラボレーション系、M5 で派生・ジョブへと拡張する。
- 公開ページ（読者ビュー）は M3 で CDN キャッシュ前提の静的化を行い、M7 で実測と調整を行う。

---

## 4. 未実装・未計画の機能

大規模な機能追加で当面実装予定がないものはここに記録する。

<a id="unplanned"></a>
- Phase 2: hub 自前バックエンドへの移行（GitHub 依存の完全除去、二重書き検証期間を経て実施）
- 通知チャネルの追加（メール・Web Push・外部チャット連携）
- セルフホスト配布物（ワンクリックデプロイ・移行ツール）
- 高度な検索と類似作品レコメンド
- プラグイン機構（独自ライセンステンプレート・独自検証ルール）
- 出所証明（コミット署名・透明性ログ・SLSA プロベナンス）
- 通知の 90 日自動パージを含む保持ポリシーの精緻化
