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
