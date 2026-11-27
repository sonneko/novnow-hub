//! ポート（trait）と hub ドメイン型の定義。
//!
//! 設計書 `REF:docs/designs/api.md` §7 に準拠する。ユースケース層はここに
//! 定義された語彙のみを知り、GitHub の概念（Repository 等）は一切知らない。
//! 依存方向は `api → usecase → ports ← adapters`。

use std::fmt;

/// 外部基盤のエラー。仕様書 §7.3 の 7 値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortError {
    /// 対象が存在しない。
    NotFound,
    /// 競合（版のずれ等）。
    Conflict,
    /// 権限不足。
    PermissionDenied,
    /// レートリミット。
    RateLimited,
    /// 外部基盤が利用できない（タイムアウト・5xx 等）。
    Unavailable,
    /// 外部基盤の応答が想定と乖離している。
    InvalidResponse,
    /// その他の未知のエラー。
    Unknown,
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PortError::NotFound => "not_found",
            PortError::Conflict => "conflict",
            PortError::PermissionDenied => "permission_denied",
            PortError::RateLimited => "rate_limited",
            PortError::Unavailable => "unavailable",
            PortError::InvalidResponse => "invalid_response",
            PortError::Unknown => "unknown",
        };
        write!(f, "{s}")
    }
}

impl std::error::Error for PortError {}

/// hub の利用者像（GitHub の概念を含まない）。
#[derive(Debug, Clone)]
pub struct Identity {
    /// 不透明な利用者識別子。
    pub id: String,
    /// ハンドル（@ 付きで表示する名前の本体）。
    pub handle: String,
    /// 表示名。
    pub display_name: String,
    /// アバター URL（任意）。
    pub avatar_url: Option<String>,
}

/// 保管単位の公開範囲（hub 語彙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectVisibility {
    Public,
    Private,
    Archived,
}

/// 保管単位（リポジトリ相当）。
#[derive(Debug, Clone)]
pub struct ProjectShell {
    /// 保管単位の識別子（不透明文字列）。
    pub id: String,
    /// 既定分岐名。
    pub default_branch: String,
    /// 公開範囲。
    pub visibility: ProjectVisibility,
    /// 版識別子（ETag）。
    pub etag: String,
}

/// ツリー要素の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeEntryKind {
    File,
    Directory,
}

/// ツリーの要素 1 件。
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: String,
    pub kind: TreeEntryKind,
    pub size: Option<u64>,
    /// blob 単位の版識別子。
    pub etag: String,
}

/// ファイルの符号化方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEncoding {
    /// UTF-8 テキスト。
    Text,
    /// Base64（バイナリ相当）。
    Base64,
}

/// ファイル内容と版識別子。
#[derive(Debug, Clone)]
pub struct FileContent {
    pub content: String,
    pub encoding: FileEncoding,
    pub etag: String,
}

/// コミット対象の新規ファイル。
#[derive(Debug, Clone)]
pub struct NewFile {
    pub path: String,
    pub content: String,
    pub encoding: FileEncoding,
}

/// 外部サービスの利用者像とアクセス権への交換。
pub trait IdentityPort: Send + Sync {
    /// 認可コードを外部サービスの利用者像へ交換する（PKCE の code_verifier を渡す）。
    fn exchange_code_for_identity(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> impl std::future::Future<Output = Result<Identity, PortError>> + Send;

    /// OAuth 許可を失効させる。
    fn revoke_grant(
        &self,
        token: &str,
    ) -> impl std::future::Future<Output = Result<(), PortError>> + Send;
}

/// 保管単位の作成・参照・公開範囲切り替え。
pub trait ProjectStoragePort: Send + Sync {
    /// 空の保管単位を作成する。
    fn create_project_shell(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> impl std::future::Future<Output = Result<ProjectShell, PortError>> + Send;

    /// 保管単位のメタ（公開範囲・既定分岐・ETag）を取得する。
    fn get_project_meta(
        &self,
        shell_id: &str,
    ) -> impl std::future::Future<Output = Result<ProjectShell, PortError>> + Send;

    /// 公開／非公開を切り替える。
    fn set_visibility(
        &self,
        shell_id: &str,
        visibility: ProjectVisibility,
    ) -> impl std::future::Future<Output = Result<(), PortError>> + Send;

    /// 保管単位を取り下げる。
    fn delete_project_shell(
        &self,
        shell_id: &str,
    ) -> impl std::future::Future<Output = Result<(), PortError>> + Send;
}

/// コンテンツ（ツリー・ファイル・コミット）の操作。
pub trait ContentPort: Send + Sync {
    /// ファイルツリーを取得する。
    fn list_tree(
        &self,
        shell_id: &str,
        path: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Vec<TreeEntry>, PortError>> + Send;

    /// ファイル内容と ETag を取得する。
    fn read_file(
        &self,
        shell_id: &str,
        path: &str,
    ) -> impl std::future::Future<Output = Result<FileContent, PortError>> + Send;

    /// 複数ファイルの原子的コミットを作成する。返り値はコミット識別子。
    fn commit_files(
        &self,
        shell_id: &str,
        branch: &str,
        files: &[NewFile],
    ) -> impl std::future::Future<Output = Result<String, PortError>> + Send;

    /// ファイル／プロジェクトの版識別子を取得する。
    fn get_etag(
        &self,
        shell_id: &str,
        path: Option<&str>,
    ) -> impl std::future::Future<Output = Result<String, PortError>> + Send;
}

// ============================================================================
// M4 コラボレーション（設計 REF:docs/designs/m4.md §2・§3・§4）。
// 変更提案・課題・自動マージ・チェック状態を操作するポート。GitHub の概念
// （Pull Request・Issue 等）はここに漏れず、hub の語彙のみで記述する。
// ============================================================================

/// 変更提案の状態（hub 語彙、契約 `Proposal.state`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    /// オープン。
    Open,
    /// 承認済み。
    Approved,
    /// 取り込み済み。
    Merged,
    /// 却下・クローズ。
    Closed,
}

/// チェックの状態（hub 語彙、契約 `Check.state`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Pending,
    Success,
    Failure,
    Neutral,
    Skipped,
}

/// 1 件のチェック状態。
#[derive(Debug, Clone)]
pub struct CheckStatus {
    pub name: String,
    pub state: CheckState,
    pub details_url: Option<String>,
}

/// 差分統計（契約 `DiffStats`）。
#[derive(Debug, Clone, Copy, Default)]
pub struct DiffStats {
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
}

/// 自動マージの状態（hub 語彙、契約 `AutomergeState.state`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomergeStateKind {
    None,
    Reserved,
    Armed,
    Merged,
    Failed,
    Canceled,
}

/// 自動マージの予約状態。
#[derive(Debug, Clone)]
pub struct AutomergeState {
    pub state: AutomergeStateKind,
    pub required_approvals: u64,
    pub required_checks: Vec<String>,
    pub require_no_conflicts: bool,
    pub reason: Option<String>,
    /// If-Match 用の版識別子。
    pub etag: String,
}

/// 自動マージの予約条件。
#[derive(Debug, Clone)]
pub struct AutomergeConfig {
    pub required_approvals: u64,
    pub required_checks: Vec<String>,
    pub require_no_conflicts: bool,
}

/// 変更提案（契約 `Proposal`）。
#[derive(Debug, Clone)]
pub struct Proposal {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: ProposalState,
    pub author_handle: String,
    pub base_ref: String,
    pub head_ref: String,
    pub diff_stats: DiffStats,
    pub checks: Vec<CheckStatus>,
    pub approval_count: u64,
    pub required_approvals: u64,
    pub automerge: Option<AutomergeState>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 差分のファイル状態（契約 `ProposalDiff.files[].status`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// 差分の 1 ファイル。
#[derive(Debug, Clone)]
pub struct ProposalFile {
    pub path: String,
    pub status: FileStatus,
    pub additions: u64,
    pub deletions: u64,
    pub hunks: Vec<String>,
}

/// novnow 診断差分（契約 `ProposalDiff.novnowDiagnosticsDelta`）。
#[derive(Debug, Clone, Copy, Default)]
pub struct DiagnosticsDelta {
    pub introduced: u64,
    pub resolved: u64,
}

/// 変更提案の差分（契約 `ProposalDiff`）。
#[derive(Debug, Clone)]
pub struct ProposalDiff {
    pub number: u64,
    pub files: Vec<ProposalFile>,
    pub novnow_diagnostics_delta: DiagnosticsDelta,
}

/// コメント（契約 `Comment`）。インラインコメントは path・line を伴う。
#[derive(Debug, Clone)]
pub struct Comment {
    pub id: String,
    pub author_handle: String,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub created_at: i64,
}

/// レビューの判定（契約 `reviewProposal.verdict`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Approve,
    Dismiss,
    RequestChanges,
}

/// マージ結果（契約 `MergeResult`）。
#[derive(Debug, Clone)]
pub struct MergeResult {
    pub merged: bool,
    pub sha: String,
    pub merged_at: Option<i64>,
}

/// 課題の状態（hub 語彙、契約 `Issue.state`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Open,
    Closed,
}

/// 課題（契約 `Issue`）。
#[derive(Debug, Clone)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: IssueState,
    pub labels: Vec<String>,
    pub author_handle: String,
    pub assignee_handle: Option<String>,
    pub linked_proposal_number: Option<u64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 課題の更新内容（契約 `IssueUpdate`）。
#[derive(Debug, Clone, Default)]
pub struct IssueUpdate {
    pub title: Option<String>,
    pub body: Option<String>,
    pub labels: Option<Vec<String>>,
    pub assignee_handle: Option<Option<String>>,
    pub state: Option<IssueState>,
}

/// 変更提案の操作（設計 §2.1）。GitHub の Pull Request を代理操作する。
pub trait ProposalPort: Send + Sync {
    fn open_proposal(
        &self,
        shell_id: &str,
        title: &str,
        body: &str,
        head_ref: &str,
        base_ref: &str,
    ) -> impl std::future::Future<Output = Result<Proposal, PortError>> + Send;

    fn get_proposal(
        &self,
        shell_id: &str,
        number: u64,
    ) -> impl std::future::Future<Output = Result<Proposal, PortError>> + Send;

    fn list_proposals(
        &self,
        shell_id: &str,
        state: Option<ProposalState>,
    ) -> impl std::future::Future<Output = Result<Vec<Proposal>, PortError>> + Send;

    fn diff_proposal(
        &self,
        shell_id: &str,
        number: u64,
    ) -> impl std::future::Future<Output = Result<ProposalDiff, PortError>> + Send;

    fn comment_proposal(
        &self,
        shell_id: &str,
        number: u64,
        body: &str,
        path: Option<&str>,
        line: Option<u64>,
    ) -> impl std::future::Future<Output = Result<Comment, PortError>> + Send;

    fn review_proposal(
        &self,
        shell_id: &str,
        number: u64,
        verdict: ReviewVerdict,
        body: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Proposal, PortError>> + Send;

    fn merge_proposal(
        &self,
        shell_id: &str,
        number: u64,
    ) -> impl std::future::Future<Output = Result<MergeResult, PortError>> + Send;

    fn close_proposal(
        &self,
        shell_id: &str,
        number: u64,
    ) -> impl std::future::Future<Output = Result<Proposal, PortError>> + Send;

    fn list_proposal_comments(
        &self,
        shell_id: &str,
        number: u64,
    ) -> impl std::future::Future<Output = Result<Vec<Comment>, PortError>> + Send;
}

/// 課題の操作（設計 §3.1）。GitHub の Issue を代理操作する。
pub trait IssuePort: Send + Sync {
    fn open_issue(
        &self,
        shell_id: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> impl std::future::Future<Output = Result<Issue, PortError>> + Send;

    fn get_issue(
        &self,
        shell_id: &str,
        number: u64,
    ) -> impl std::future::Future<Output = Result<Issue, PortError>> + Send;

    fn list_issues(
        &self,
        shell_id: &str,
        state: Option<IssueState>,
        label: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Vec<Issue>, PortError>> + Send;

    fn update_issue(
        &self,
        shell_id: &str,
        number: u64,
        update: &IssueUpdate,
    ) -> impl std::future::Future<Output = Result<Issue, PortError>> + Send;

    fn comment_issue(
        &self,
        shell_id: &str,
        number: u64,
        body: &str,
    ) -> impl std::future::Future<Output = Result<Comment, PortError>> + Send;

    fn close_issue(
        &self,
        shell_id: &str,
        number: u64,
        state: IssueState,
    ) -> impl std::future::Future<Output = Result<Issue, PortError>> + Send;

    fn list_issue_comments(
        &self,
        shell_id: &str,
        number: u64,
    ) -> impl std::future::Future<Output = Result<Vec<Comment>, PortError>> + Send;
}

/// 自動マージの操作（設計 §4.1）。GitHub の自動マージ機構を代理操作する。
pub trait AutomergePort: Send + Sync {
    fn reserve_automerge(
        &self,
        shell_id: &str,
        number: u64,
        config: &AutomergeConfig,
    ) -> impl std::future::Future<Output = Result<AutomergeState, PortError>> + Send;

    fn cancel_automerge(
        &self,
        shell_id: &str,
        number: u64,
    ) -> impl std::future::Future<Output = Result<(), PortError>> + Send;

    fn get_automerge_state(
        &self,
        shell_id: &str,
        number: u64,
    ) -> impl std::future::Future<Output = Result<AutomergeState, PortError>> + Send;
}

/// チェック状態の取得（設計 §2.2）。GitHub の Check Runs／Statuses を代理する。
pub trait CheckStatusPort: Send + Sync {
    /// 指定リファレンスのチェック状態を返す。
    fn get_check_status(
        &self,
        shell_id: &str,
        ref_name: &str,
    ) -> impl std::future::Future<Output = Result<Vec<CheckStatus>, PortError>> + Send;
}

/// 通知の種別（契約 `Notification.kind` の hub 側語彙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    DerivativeCreated,
    ProposalMerged,
    ProposalAutomergeFailed,
    ProposalComment,
    IssueCreated,
    IssueComment,
    Mention,
    JobFinished,
    PublishRebuilt,
}

impl NotificationKind {
    /// 契約 `Notification.kind` の文字列表記。
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationKind::DerivativeCreated => "derivative_created",
            NotificationKind::ProposalMerged => "proposal_merged",
            NotificationKind::ProposalAutomergeFailed => "proposal_automerge_failed",
            NotificationKind::ProposalComment => "proposal_comment",
            NotificationKind::IssueCreated => "issue_created",
            NotificationKind::IssueComment => "issue_comment",
            NotificationKind::Mention => "mention",
            NotificationKind::JobFinished => "job_finished",
            NotificationKind::PublishRebuilt => "publish_rebuilt",
        }
    }
}

/// 通知（hub 管理ストアに保存。契約 `Notification` に対応）。
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: String,
    pub kind: NotificationKind,
    pub project_id: Option<String>,
    pub project_slug: Option<String>,
    pub title: String,
    pub body: String,
    pub read: bool,
    /// 発生時刻（epoch 秒）。
    pub created_at: i64,
}

/// 通知の取得・既読化・ファンアウト（設計 §6）。hub 管理ストアで実装する。
pub trait NotificationPort: Send + Sync {
    /// 通知一覧を返す（カーソルページネーション）。
    fn list_notifications(
        &self,
        user_id: &str,
        kind: Option<NotificationKind>,
        unread_only: bool,
        cursor: Option<&str>,
        limit: usize,
    ) -> impl std::future::Future<Output = Result<(Vec<Notification>, Option<String>), PortError>> + Send;

    /// 指定 ID を既読化、または全既読化する。
    fn mark_read(
        &self,
        user_id: &str,
        ids: &[String],
        all: bool,
    ) -> impl std::future::Future<Output = Result<(), PortError>> + Send;

    /// ドメインイベントから通知をファンアウトする（設計 §6.2）。
    fn emit(
        &self,
        user_id: &str,
        kind: NotificationKind,
        project_id: Option<&str>,
        project_slug: Option<&str>,
        title: &str,
        body: &str,
    ) -> impl std::future::Future<Output = Result<(), PortError>> + Send;
}

/// Webhook イベントの種別（hub 側語彙）。
///
/// GitHub の概念（イベント名等）はアダプター層で吸収し、ここには hub 語彙のみ
/// を置く（設計 §7.1「アダプター層の背後に GitHub の概念を隠す」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookEventKind {
    /// ブランチへの推送。公開ページの再生成対象。
    Push,
    /// その他のイベント（再生成対象外。受信自体は認める）。
    Other,
}

/// 受信した Webhook イベント（hub 語彙）。
///
/// `repo_full_name` は保管単位の識別子（shell_id 相当）で、登録簿の
/// `get_by_shell` でプロジェクトへ解決する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookEvent {
    pub kind: WebhookEventKind,
    pub repo_full_name: String,
}

/// 公開ページの再生成とキャッシュ失効（設計 §7.1）。
///
/// ドメイン側は「対象プロジェクトの公開ページを再生成し、キャッシュを失効
/// させる」ことだけを知る。実際の再生成・CDN 失効の実装は適応層（この
/// hub では hub 管理のロギング実装）に隔離する。
///
/// `dyn` で利用可能にするため、返り値はボックス化 future とする。
pub trait RebuildPort: Send + Sync {
    /// 対象プロジェクトの公開ページを再生成し、キャッシュを失効させる。
    fn rebuild_public_page(
        &self,
        project_id: &str,
        repo: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), PortError>> + Send + '_>,
    >;
}

/// 派生可否の方針（契約 `derivativePolicy` の hub 側語彙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivativePolicy {
    /// 派生可。
    Allowed,
    /// 条件付きで派生可。
    AllowedWithConditions,
    /// 派生不可。
    Disallowed,
}

impl DerivativePolicy {
    /// 契約 `derivativePolicy` の文字列表記。
    pub fn as_str(&self) -> &'static str {
        match self {
            DerivativePolicy::Allowed => "allowed",
            DerivativePolicy::AllowedWithConditions => "allowed-with-conditions",
            DerivativePolicy::Disallowed => "disallowed",
        }
    }

    /// 文字列からポリシーへ変換（未知は `Disallowed` 扱い）。
    pub fn from_policy_str(raw: &str) -> Self {
        match raw {
            "allowed" => DerivativePolicy::Allowed,
            "allowed-with-conditions" => DerivativePolicy::AllowedWithConditions,
            _ => DerivativePolicy::Disallowed,
        }
    }
}

/// 派生作品の記録（hub 管理。契約 `DerivativeNode` に対応）。
///
/// `parent_id` は派生元のプロジェクト ID。`depth` は派生元からの距離
/// （直接の子は 1、孫は 2…）。派生の派生（孫作品）も同様に記録し、
/// 派生元の連鎖を保持する。
#[derive(Debug, Clone)]
pub struct Derivative {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub owner_handle: String,
    /// 派生元からの距離（直接の子は 1）。
    pub depth: u32,
    /// 派生元のプロジェクト ID（派生元が無ければ None）。
    pub parent_id: Option<String>,
    /// 継承したライセンス。
    pub license: String,
    /// 派生可否の方針。
    pub derivative_policy: DerivativePolicy,
    /// 作成日時（epoch 秒）。
    pub created_at: i64,
    /// 更新日時（epoch 秒）。
    pub updated_at: i64,
}

/// 派生作成の要求（契約 `deriveProject` のリクエストボディ）。
#[derive(Debug, Clone)]
pub struct DeriveRequest {
    /// 作品タイトル（必須）。
    pub title: String,
    /// 説明（任意）。
    pub description: Option<String>,
    /// 公開範囲（public / private、既定 private）。
    pub visibility: ProjectVisibility,
}

/// 派生作成の受理結果（契約 `JobAccepted` に対応）。
#[derive(Debug, Clone)]
pub struct DeriveAccepted {
    pub job_id: String,
    pub status: JobStatus,
}

/// 派生ジョブの状態（契約 `JobAccepted.status`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
        }
    }
}

/// 派生の作成と派生ツリーの取得（設計 §2.2）。
///
/// Phase 1 の実装は GitHub の Fork を代理操作するアダプターであり、派生元との
/// 対応関係（parent_id / depth）は hub 側で記録する。GitHub の概念（Fork 等）
/// はアダプター層の背後に隠し、API 契約・フロントエンドには漏らさない。
pub trait DerivationPort: Send + Sync {
    /// 派生作品を作成する（非同期）。Fork 完了を待って hub に派生レコードを
    /// 作成し、完了を通知で知らせる。派生不可の場合は 409（Conflict）。
    ///
    /// `actor_id` / `actor_handle` は派生を実行した利用者（派生作品の作者）。
    fn create_derivative(
        &self,
        parent_project_id: &str,
        actor_id: &str,
        actor_handle: &str,
        req: &DeriveRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<DeriveAccepted, PortError>> + Send + '_,
        >,
    >;

    /// 指定プロジェクトの派生ツリー（子・孫・兄弟・派生元）を返す。
    fn list_derivatives(
        &self,
        project_id: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<Derivative>, PortError>> + Send + '_>,
    >;

    /// 指定プロジェクトの派生ツリーを返す（`list_derivatives` と同義）。
    fn get_derivation_tree(
        &self,
        project_id: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<Derivative>, PortError>> + Send + '_>,
    >;

    /// 指定派生作品を取得する。
    fn get_derivative(
        &self,
        project_id: &str,
        derivative_id: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Derivative, PortError>> + Send + '_>,
    >;
}

// ============================================================================
// M5 novnow 診断・エクスポートの非同期ジョブ基盤（設計 REF:docs/designs/m5.md §4）。
// ジョブの種別は診断（novnow check 相当）とエクスポート（注釈除去済み
// テキスト／Markdown）。GitHub の概念（GitHub Actions 等）はポートの背後に
// 隠し、API 契約・フロントエンドには漏らさない。
// ============================================================================

/// ジョブの種別（契約 `Job.kind` の列挙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// novnow 形式の妥当性検証（M3 軽量検証相当。将来の本格化用）。
    Verify,
    /// novnow 診断（novnow check 相当）。
    Diagnostics,
    /// 本文のエクスポート（注釈除去済みテキスト／Markdown）。
    Export,
}

impl JobKind {
    /// 契約 `Job.kind` の文字列表記。
    pub fn as_str(&self) -> &'static str {
        match self {
            JobKind::Verify => "verify",
            JobKind::Diagnostics => "diagnostics",
            JobKind::Export => "export",
        }
    }

    /// 文字列から種別へ変換（未知は `Diagnostics` 扱い）。
    pub fn from_job_kind_str(raw: &str) -> Self {
        match raw {
            "verify" => JobKind::Verify,
            "export" => JobKind::Export,
            _ => JobKind::Diagnostics,
        }
    }
}

/// ジョブの状態（ポート語彙）。
///
/// ポート側は `Cancelled` を持つが、契約 `Job.status` の列挙
/// （`pending`/`running`/`succeeded`/`failed`）に `cancelled` は無い。
/// API 境界では `Cancelled` を `failed`（理由「キャンセルされました」）に
/// 写像する（権威の順位: 契約を正とする。判断は `docs/handoff.md` §4 に記録）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// 起票済み（未実行）。
    Pending,
    /// 実行中。
    Running,
    /// 成功。
    Succeeded,
    /// 失敗（理由を残す）。
    Failed,
    /// キャンセル。
    Cancelled,
}

impl JobState {
    /// ポート語彙の文字列表記。
    pub fn as_str(&self) -> &'static str {
        match self {
            JobState::Pending => "pending",
            JobState::Running => "running",
            JobState::Succeeded => "succeeded",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
        }
    }

    /// 契約 `Job.status` の文字列表記（`Cancelled` は `failed` に写像）。
    pub fn as_contract_str(&self) -> &'static str {
        match self {
            JobState::Cancelled => "failed",
            other => other.as_str(),
        }
    }

    /// 完了（成功・失敗・キャンセル）状態か。
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            JobState::Succeeded | JobState::Failed | JobState::Cancelled
        )
    }
}

/// エクスポートの出力形式（契約 `createExport` の `format` 列挙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// 注釈除去済みテキスト。
    Txt,
    /// Markdown。
    Markdown,
}

impl ExportFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExportFormat::Txt => "txt",
            ExportFormat::Markdown => "markdown",
        }
    }

    pub fn from_export_format_str(raw: &str) -> Self {
        match raw {
            "markdown" => ExportFormat::Markdown,
            _ => ExportFormat::Txt,
        }
    }
}

/// 診断結果の 1 件（契約 `VerificationResult` の errors/warnings 要素）。
#[derive(Debug, Clone)]
pub struct DiagnosticItem {
    pub path: String,
    pub code: String,
    pub message: String,
}

/// 診断（novnow check 相当）の結果。
#[derive(Debug, Clone)]
pub struct DiagnosticsResult {
    pub ok: bool,
    pub errors: Vec<DiagnosticItem>,
    pub warnings: Vec<DiagnosticItem>,
    /// 検査時刻（epoch 秒）。
    pub checked_at: i64,
}

/// エクスポートの結果。
#[derive(Debug, Clone)]
pub struct ExportResult {
    /// 出力形式。
    pub format: String,
    /// 生成した本文の文字数（サマリ）。
    pub char_count: usize,
}

/// ジョブの結果（種別ごと）。
#[derive(Debug, Clone)]
pub enum JobResult {
    Diagnostics(DiagnosticsResult),
    Export(ExportResult),
}

/// 非同期ジョブの記録（契約 `Job` に対応）。
#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub status: JobState,
    pub result: Option<JobResult>,
    pub error: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

/// ジョブ作成の要求。
#[derive(Debug, Clone)]
pub struct CreateJobRequest {
    pub kind: JobKind,
    pub project_id: String,
    /// エクスポートジョブの出力形式（診断ジョブでは無視）。
    pub format: Option<ExportFormat>,
}

/// novnow 診断・エクスポートの非同期ジョブ基盤（設計 §4.1）。
///
/// ジョブはキュー化され、同時実行数を制限して実行される。完了は通知
/// （`job_finished`）で知らせる。GitHub の概念（GitHub Actions 等）は
/// 実装側の背後に隠し、API 契約・フロントエンドには漏らさない。
pub trait JobPort: Send + Sync {
    /// ジョブを起票し、非同期で実行する。完了時に通知を発する。
    fn create_job(
        &self,
        req: &CreateJobRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Job, PortError>> + Send + '_>,
    >;

    /// 指定ジョブを取得する。
    fn get_job(
        &self,
        job_id: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Job, PortError>> + Send + '_>,
    >;

    /// ジョブ一覧を返す。
    fn list_jobs(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<Job>, PortError>> + Send + '_>,
    >;

    /// ジョブをキャンセルする（完了済みはキャンセルできない）。
    fn cancel_job(
        &self,
        job_id: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Job, PortError>> + Send + '_>,
    >;
}

// ============================================================================
// Cloudflare D1（SQLite）バインディング用ポート（M7 §2.1、Worker ランタイム適応）。
//
// Phase 1 ではインメモリストア＋GitHub を主backとするため、D1 への実接続は
// アカウント側の D1 バインディング設定待ち（判断 REF:docs/handoff.md §4 判断 26）。
// ここではコード側のインターフェースのみを整え、実接続は行わない。
// ============================================================================

/// Cloudflare D1 データベースへのアクセスポート。
///
/// 実接続は Cloudflare アカウント側で D1 データベースを作成し、wrangler.toml の
/// `[[d1_databases]]` でバインディングを設定した後、アダプター側で実装する。
/// 本リポジトリでは実接続を行わず、トレイト定義（コード側の準備）に留める。
pub trait D1Port: Send + Sync {
    /// SQL を実行し、結果を行のリスト（列名 → 値 のマップ）で返す。
    ///
    /// `params` はプレースホルダ（`?`）にバインドする値。
    fn query(
        &self,
        sql: &str,
        params: &[&dyn std::fmt::Display],
    ) -> impl std::future::Future<
        Output = Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            PortError,
        >,
    > + Send;

    /// SQL を実行し、影響行数を返す。
    fn execute(
        &self,
        sql: &str,
        params: &[&dyn std::fmt::Display],
    ) -> impl std::future::Future<Output = Result<u64, PortError>> + Send;
}
