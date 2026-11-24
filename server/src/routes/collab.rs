//! M4 コラボレーションの HTTP ルーター。
//!
//! 変更提案（ProposalPort）・課題（IssuePort）・自動マージ（AutomergePort）を
//! API 契約 `REF:api/openapi.yaml` のエンドポイントに配線する。GitHub の概念
//! は本層に漏らさず、すべてポート経由（`ProposalPort`・`IssuePort`・
//! `AutomergePort`）でアクセスする。
//!
//! 認可は既存の `require_auth`／公開範囲の扱いに合わせる。契約で `security: []`
//! の読み取り（変更提案・課題・自動マージ状態の取得）は公開、書き込み系は
//! 認証必須とする（ルーター側で `require_auth` ミドルウェアを適用する）。

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::middleware::RequestIdentity;
use crate::ports::{
    AutomergeConfig, AutomergePort, AutomergeState, AutomergeStateKind, CheckState, Comment,
    FileStatus, Issue, IssuePort, IssueState, IssueUpdate, MergeResult, NotificationKind,
    NotificationPort, PortError, Proposal, ProposalDiff, ProposalPort, ProposalState, ReviewVerdict,
};

// ---------------------------------------------------------------------------
// ヘルパー
// ---------------------------------------------------------------------------

/// UNIX 秒を ISO8601（UTC）へ変換。
fn iso8601(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// projectId を保管単位名（shell_id）へ解決する。未登録なら 404。
fn resolve_shell(state: &AppState, project_id: &str) -> Result<String, ApiError> {
    state
        .projects
        .get(project_id)
        .map(|r| r.shell_id)
        .ok_or_else(|| ApiError::NotFound(format!("プロジェクトが見つかりません: {project_id}")))
}

/// 認証済みのアクター（利用者 ID）を取得する。
fn actor_user_id(identity: &RequestIdentity) -> Result<String, ApiError> {
    identity
        .session
        .identity
        .as_ref()
        .map(|u| u.id.clone())
        .ok_or_else(|| ApiError::AuthenticationRequired("利用者の情報がありません".into()))
}

/// コメント投稿のファンアウト（契約 `proposal_comment`／`issue_comment`）。
async fn fanout_comment(
    state: &AppState,
    identity: &RequestIdentity,
    project_id: &str,
    shell_id: &str,
    number: u64,
    is_proposal: bool,
) -> Result<(), ApiError> {
    let actor = actor_user_id(identity)?;
    let (kind, label) = if is_proposal {
        (NotificationKind::ProposalComment, "変更提案")
    } else {
        (NotificationKind::IssueComment, "課題")
    };
    state
        .notifications
        .emit(
            &actor,
            kind,
            Some(project_id),
            Some(shell_id),
            &format!("{label} #{number} にコメント"),
            "コメントを投稿しました。",
        )
        .await
        .map_err(map_port_error)?;
    Ok(())
}

/// 取り込みのファンアウト（契約 `proposal_merged`）。
async fn fanout_merge(
    state: &AppState,
    identity: &RequestIdentity,
    project_id: &str,
    shell_id: &str,
    number: u64,
) -> Result<(), ApiError> {
    let actor = actor_user_id(identity)?;
    state
        .notifications
        .emit(
            &actor,
            NotificationKind::ProposalMerged,
            Some(project_id),
            Some(shell_id),
            &format!("変更提案 #{number} が取り込まれました"),
            "提案がマージされました。",
        )
        .await
        .map_err(map_port_error)?;
    Ok(())
}

/// 課題作成のファンアウト（契約 `issue_created`）。
async fn fanout_issue(
    state: &AppState,
    identity: &RequestIdentity,
    project_id: &str,
    shell_id: &str,
    action: &str,
    title: &str,
) -> Result<(), ApiError> {
    let actor = actor_user_id(identity)?;
    state
        .notifications
        .emit(
            &actor,
            NotificationKind::IssueCreated,
            Some(project_id),
            Some(shell_id),
            &format!("課題「{title}」を{action}しました"),
            "課題を操作しました。",
        )
        .await
        .map_err(map_port_error)?;
    Ok(())
}

/// ポートエラーを API エラーへ写像する。
fn map_port_error(e: PortError) -> ApiError {
    match e {
        PortError::NotFound => ApiError::NotFound("対象が見つかりません".into()),
        PortError::Conflict => ApiError::Conflict("競合が発生しました".into()),
        PortError::PermissionDenied => ApiError::PermissionDenied("権限が不足しています".into()),
        PortError::RateLimited => ApiError::RateLimited("レートリミットに達しました".into()),
        PortError::Unavailable => ApiError::UpstreamUnavailable("外部基盤が利用できません".into()),
        PortError::InvalidResponse => ApiError::Internal("外部基盤の応答が不正です".into()),
        PortError::Unknown => ApiError::Internal("不明なエラー".into()),
    }
}

/// 一覧の既定ページサイズ（契約 `limit` の既定）。
const DEFAULT_PAGE_LIMIT: usize = 20;
/// 一覧のページサイズ上限（契約 `limit` の上限）。
const MAX_PAGE_LIMIT: usize = 100;

/// `limit` クエリを検証してページサイズへ解決する。
fn resolve_limit(raw: Option<u32>) -> Result<usize, ApiError> {
    match raw {
        None => Ok(DEFAULT_PAGE_LIMIT),
        Some(n) => {
            if n < 1 || n > MAX_PAGE_LIMIT as u32 {
                Err(ApiError::ValidationFailed(format!(
                    "limit は 1 以上 {MAX_PAGE_LIMIT} 以下にしてください（{n}）"
                )))
            } else {
                Ok(n as usize)
            }
        }
    }
}

/// カーソル（オフセット）を解決する。
fn resolve_cursor(raw: Option<&str>) -> Result<usize, ApiError> {
    match raw {
        None => Ok(0),
        Some(c) => {
            if c.chars().count() > 512 {
                return Err(ApiError::ValidationFailed("カーソルが長すぎます".into()));
            }
            c.parse::<usize>()
                .map_err(|_| ApiError::ValidationFailed("カーソルが不正です".into()))
        }
    }
}

/// カーソル（オフセット）でページを切り出す。次ページがあれば nextCursor を返す。
fn paginate<T: Clone>(items: &[T], cursor: usize, limit: usize) -> Result<(Vec<T>, Option<String>), ApiError> {
    if cursor > items.len() {
        return Err(ApiError::ValidationFailed("カーソルが範囲外です".into()));
    }
    let end = (cursor + limit).min(items.len());
    let page_items = items[cursor..end].to_vec();
    let next = if end < items.len() {
        Some(end.to_string())
    } else {
        None
    };
    Ok((page_items, next))
}

/// hub の列挙値を契約の文字列へ写す。
fn proposal_state_str(s: ProposalState) -> &'static str {
    match s {
        ProposalState::Open => "open",
        ProposalState::Approved => "approved",
        ProposalState::Merged => "merged",
        ProposalState::Closed => "closed",
    }
}

fn check_state_str(s: CheckState) -> &'static str {
    match s {
        CheckState::Pending => "pending",
        CheckState::Success => "success",
        CheckState::Failure => "failure",
        CheckState::Neutral => "neutral",
        CheckState::Skipped => "skipped",
    }
}

fn automerge_state_str(s: AutomergeStateKind) -> &'static str {
    match s {
        AutomergeStateKind::None => "none",
        AutomergeStateKind::Reserved => "reserved",
        AutomergeStateKind::Armed => "armed",
        AutomergeStateKind::Merged => "merged",
        AutomergeStateKind::Failed => "failed",
        AutomergeStateKind::Canceled => "canceled",
    }
}

fn issue_state_str(s: IssueState) -> &'static str {
    match s {
        IssueState::Open => "open",
        IssueState::Closed => "closed",
    }
}

fn file_status_str(s: FileStatus) -> &'static str {
    match s {
        FileStatus::Added => "added",
        FileStatus::Modified => "modified",
        FileStatus::Deleted => "deleted",
        FileStatus::Renamed => "renamed",
    }
}

fn parse_proposal_state(raw: &str) -> Result<ProposalState, ApiError> {
    match raw {
        "open" => Ok(ProposalState::Open),
        "approved" => Ok(ProposalState::Approved),
        "merged" => Ok(ProposalState::Merged),
        "closed" => Ok(ProposalState::Closed),
        other => Err(ApiError::ValidationFailed(format!(
            "state が不正です: {other}"
        ))),
    }
}

fn parse_issue_state(raw: &str) -> Result<IssueState, ApiError> {
    match raw {
        "open" => Ok(IssueState::Open),
        "closed" => Ok(IssueState::Closed),
        other => Err(ApiError::ValidationFailed(format!(
            "state が不正です: {other}"
        ))),
    }
}

fn parse_review_verdict(raw: &str) -> Result<ReviewVerdict, ApiError> {
    match raw {
        "approve" => Ok(ReviewVerdict::Approve),
        "dismiss" => Ok(ReviewVerdict::Dismiss),
        "request_changes" => Ok(ReviewVerdict::RequestChanges),
        other => Err(ApiError::ValidationFailed(format!(
            "verdict が不正です: {other}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// レスポンス型（契約スキーマに準拠、camelCase）
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResponse {
    name: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffStatsResponse {
    additions: u64,
    deletions: u64,
    changed_files: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomergeStateResponse {
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_approvals: Option<u64>,
    required_checks: Vec<String>,
    require_no_conflicts: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalResponse {
    number: u64,
    title: String,
    body: String,
    state: String,
    author_handle: String,
    base_ref: String,
    head_ref: String,
    diff_stats: DiffStatsResponse,
    checks: Vec<CheckResponse>,
    approval_count: u64,
    required_approvals: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    automerge: Option<AutomergeStateResponse>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalPageResponse {
    items: Vec<ProposalResponse>,
    next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileResponse {
    path: String,
    status: String,
    additions: u64,
    deletions: u64,
    hunks: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsDeltaResponse {
    introduced: u64,
    resolved: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDiffResponse {
    number: u64,
    files: Vec<DiffFileResponse>,
    novnow_diagnostics_delta: DiagnosticsDeltaResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentResponse {
    id: String,
    author_handle: String,
    body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u64>,
    created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentPageResponse {
    items: Vec<CommentResponse>,
    next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResultResponse {
    merged: bool,
    sha: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    merged_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueResponse {
    number: u64,
    title: String,
    body: String,
    state: String,
    labels: Vec<String>,
    author_handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee_handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    linked_proposal_number: Option<u64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuePageResponse {
    items: Vec<IssueResponse>,
    next_cursor: Option<String>,
}

// ---------------------------------------------------------------------------
// 写像ヘルパー
// ---------------------------------------------------------------------------

fn to_check_response(c: &crate::ports::CheckStatus) -> CheckResponse {
    CheckResponse {
        name: c.name.clone(),
        state: check_state_str(c.state).to_string(),
        details_url: c.details_url.clone(),
    }
}

fn to_automerge_response(a: &AutomergeState) -> AutomergeStateResponse {
    AutomergeStateResponse {
        state: automerge_state_str(a.state).to_string(),
        required_approvals: Some(a.required_approvals),
        required_checks: a.required_checks.clone(),
        require_no_conflicts: a.require_no_conflicts,
        reason: a.reason.clone(),
        etag: Some(a.etag.clone()),
    }
}

fn to_proposal_response(p: &Proposal) -> ProposalResponse {
    ProposalResponse {
        number: p.number,
        title: p.title.clone(),
        body: p.body.clone(),
        state: proposal_state_str(p.state).to_string(),
        author_handle: p.author_handle.clone(),
        base_ref: p.base_ref.clone(),
        head_ref: p.head_ref.clone(),
        diff_stats: DiffStatsResponse {
            additions: p.diff_stats.additions,
            deletions: p.diff_stats.deletions,
            changed_files: p.diff_stats.changed_files,
        },
        checks: p.checks.iter().map(to_check_response).collect(),
        approval_count: p.approval_count,
        required_approvals: p.required_approvals,
        automerge: p.automerge.as_ref().map(to_automerge_response),
        created_at: iso8601(p.created_at),
        updated_at: iso8601(p.updated_at),
    }
}

fn to_diff_response(d: &ProposalDiff) -> ProposalDiffResponse {
    ProposalDiffResponse {
        number: d.number,
        files: d
            .files
            .iter()
            .map(|f| DiffFileResponse {
                path: f.path.clone(),
                status: file_status_str(f.status).to_string(),
                additions: f.additions,
                deletions: f.deletions,
                hunks: f.hunks.clone(),
            })
            .collect(),
        novnow_diagnostics_delta: DiagnosticsDeltaResponse {
            introduced: d.novnow_diagnostics_delta.introduced,
            resolved: d.novnow_diagnostics_delta.resolved,
        },
    }
}

fn to_comment_response(c: &Comment) -> CommentResponse {
    CommentResponse {
        id: c.id.clone(),
        author_handle: c.author_handle.clone(),
        body: c.body.clone(),
        path: c.path.clone(),
        line: c.line,
        created_at: iso8601(c.created_at),
    }
}

fn to_issue_response(i: &Issue) -> IssueResponse {
    IssueResponse {
        number: i.number,
        title: i.title.clone(),
        body: i.body.clone(),
        state: issue_state_str(i.state).to_string(),
        labels: i.labels.clone(),
        author_handle: i.author_handle.clone(),
        assignee_handle: i.assignee_handle.clone(),
        linked_proposal_number: i.linked_proposal_number,
        created_at: iso8601(i.created_at),
        updated_at: iso8601(i.updated_at),
    }
}

// ---------------------------------------------------------------------------
// リクエスト型
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListProposalsQuery {
    state: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListIssuesQuery {
    state: Option<String>,
    label: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCommentsQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProposalRequest {
    title: String,
    #[serde(default)]
    body: String,
    head_ref: String,
    base_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentProposalRequest {
    body: String,
    path: Option<String>,
    line: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewProposalRequest {
    verdict: String,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReserveAutomergeRequest {
    #[serde(default = "default_required_approvals")]
    required_approvals: u64,
    #[serde(default)]
    required_checks: Vec<String>,
    #[serde(default = "default_true")]
    require_no_conflicts: bool,
}

fn default_required_approvals() -> u64 {
    1
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenIssueRequest {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIssueRequest {
    title: Option<String>,
    body: Option<String>,
    labels: Option<Vec<String>>,
    assignee_handle: Option<Option<String>>,
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentIssueRequest {
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseIssueRequest {
    state: String,
}

// ---------------------------------------------------------------------------
// 変更提案（ProposalPort）
// ---------------------------------------------------------------------------

/// `GET /projects/{projectId}/proposals`（一覧、公開）。
pub async fn list_proposals(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Query(params): Query<ListProposalsQuery>,
) -> Result<Json<ProposalPageResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let state_filter = params.state.as_deref().map(parse_proposal_state).transpose()?;
    let limit = resolve_limit(params.limit)?;
    let cursor = resolve_cursor(params.cursor.as_deref())?;
    let proposals = state
        .github_adapter
        .list_proposals(&shell_id, state_filter)
        .await
        .map_err(map_port_error)?;
    let (page_items, next) = paginate(&proposals, cursor, limit)?;
    Ok(Json(ProposalPageResponse {
        items: page_items.iter().map(to_proposal_response).collect(),
        next_cursor: next,
    }))
}

/// `POST /projects/{projectId}/proposals`（作成、認証必須）。
pub async fn open_proposal(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(req): Json<OpenProposalRequest>,
) -> Result<(StatusCode, Json<ProposalResponse>), ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let title = req.title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(ApiError::ValidationFailed(
            "タイトルは 1〜200 文字にしてください".into(),
        ));
    }
    if req.body.chars().count() > 10000 {
        return Err(ApiError::ValidationFailed(
            "本文は 10000 文字以下にしてください".into(),
        ));
    }
    if req.head_ref.trim().is_empty() || req.head_ref.chars().count() > 255 {
        return Err(ApiError::ValidationFailed(
            "headRef は 1〜255 文字にしてください".into(),
        ));
    }
    if req.base_ref.trim().is_empty() || req.base_ref.chars().count() > 255 {
        return Err(ApiError::ValidationFailed(
            "baseRef は 1〜255 文字にしてください".into(),
        ));
    }
    let proposal = state
        .github_adapter
        .open_proposal(
            &shell_id,
            title,
            &req.body,
            req.head_ref.trim(),
            req.base_ref.trim(),
        )
        .await
        .map_err(map_port_error)?;
    Ok((StatusCode::CREATED, Json(to_proposal_response(&proposal))))
}

/// `GET /projects/{projectId}/proposals/{number}`（詳細、公開）。
pub async fn get_proposal(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
) -> Result<Json<ProposalResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let proposal = state
        .github_adapter
        .get_proposal(&shell_id, number)
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_proposal_response(&proposal)))
}

/// `GET /projects/{projectId}/proposals/{number}/diff`（差分、公開）。
pub async fn get_proposal_diff(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
) -> Result<Json<ProposalDiffResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let diff = state
        .github_adapter
        .diff_proposal(&shell_id, number)
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_diff_response(&diff)))
}

/// `GET /projects/{projectId}/proposals/{number}/comments`（コメント一覧、認証必須）。
pub async fn list_proposal_comments(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
    Query(params): Query<ListCommentsQuery>,
) -> Result<Json<CommentPageResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let limit = resolve_limit(params.limit)?;
    let cursor = resolve_cursor(params.cursor.as_deref())?;
    let comments = state
        .github_adapter
        .list_proposal_comments(&shell_id, number)
        .await
        .map_err(map_port_error)?;
    let (page_items, next) = paginate(&comments, cursor, limit)?;
    Ok(Json(CommentPageResponse {
        items: page_items.iter().map(to_comment_response).collect(),
        next_cursor: next,
    }))
}

/// `POST /projects/{projectId}/proposals/{number}/comments`（コメント、認証必須）。
pub async fn comment_proposal(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
    Extension(identity_ext): Extension<RequestIdentity>,
    Json(req): Json<CommentProposalRequest>,
) -> Result<(StatusCode, Json<CommentResponse>), ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    if req.body.trim().is_empty() || req.body.chars().count() > 10000 {
        return Err(ApiError::ValidationFailed(
            "コメント本文は 1〜10000 文字にしてください".into(),
        ));
    }
    let comment = state
        .github_adapter
        .comment_proposal(&shell_id, number, &req.body, req.path.as_deref(), req.line)
        .await
        .map_err(map_port_error)?;
    fanout_comment(&state, &identity_ext, &project_id, &shell_id, number, true).await?;
    Ok((StatusCode::CREATED, Json(to_comment_response(&comment))))
}

/// `POST /projects/{projectId}/proposals/{number}/approve`（レビュー、認証必須）。
pub async fn review_proposal(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
    Json(req): Json<ReviewProposalRequest>,
) -> Result<Json<ProposalResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let verdict = parse_review_verdict(&req.verdict)?;
    if let Some(b) = &req.body {
        if b.chars().count() > 10000 {
            return Err(ApiError::ValidationFailed(
                "本文は 10000 文字以下にしてください".into(),
            ));
        }
    }
    let proposal = state
        .github_adapter
        .review_proposal(&shell_id, number, verdict, req.body.as_deref())
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_proposal_response(&proposal)))
}

/// `POST /projects/{projectId}/proposals/{number}/merge`（取り込み、認証必須）。
pub async fn merge_proposal(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
    Extension(identity_ext): Extension<RequestIdentity>,
) -> Result<Json<MergeResultResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let result: MergeResult = state
        .github_adapter
        .merge_proposal(&shell_id, number)
        .await
        .map_err(map_port_error)?;
    fanout_merge(&state, &identity_ext, &project_id, &shell_id, number).await?;
    Ok(Json(MergeResultResponse {
        merged: result.merged,
        sha: result.sha,
        merged_at: result.merged_at.map(iso8601),
    }))
}

/// `POST /projects/{projectId}/proposals/{number}/close`（クローズ、認証必須）。
pub async fn close_proposal(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
) -> Result<Json<ProposalResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let proposal = state
        .github_adapter
        .close_proposal(&shell_id, number)
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_proposal_response(&proposal)))
}

// ---------------------------------------------------------------------------
// 自動マージ（AutomergePort）
// ---------------------------------------------------------------------------

/// `GET /projects/{projectId}/proposals/{number}/automerge`（状態、公開）。
pub async fn get_automerge_state(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
) -> Result<Json<AutomergeStateResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let am = state
        .github_adapter
        .get_automerge_state(&shell_id, number)
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_automerge_response(&am)))
}

/// `PUT /projects/{projectId}/proposals/{number}/automerge`（予約、認証必須）。
pub async fn reserve_automerge(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
    Json(req): Json<ReserveAutomergeRequest>,
) -> Result<Json<AutomergeStateResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    if req.required_approvals > 10 {
        return Err(ApiError::ValidationFailed(
            "requiredApprovals は 10 以下にしてください".into(),
        ));
    }
    let config = AutomergeConfig {
        required_approvals: req.required_approvals,
        required_checks: req.required_checks.clone(),
        require_no_conflicts: req.require_no_conflicts,
    };
    let am = state
        .github_adapter
        .reserve_automerge(&shell_id, number, &config)
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_automerge_response(&am)))
}

/// `DELETE /projects/{projectId}/proposals/{number}/automerge`（取消、認証必須）。
pub async fn cancel_automerge(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
) -> Result<Response, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    state
        .github_adapter
        .cancel_automerge(&shell_id, number)
        .await
        .map_err(map_port_error)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---------------------------------------------------------------------------
// 課題（IssuePort）
// ---------------------------------------------------------------------------

/// `GET /projects/{projectId}/issues`（一覧、公開）。
pub async fn list_issues(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Query(params): Query<ListIssuesQuery>,
) -> Result<Json<IssuePageResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let state_filter = params.state.as_deref().map(parse_issue_state).transpose()?;
    if let Some(l) = &params.label {
        if l.chars().count() > 100 {
            return Err(ApiError::ValidationFailed(
                "ラベルは 100 文字以下にしてください".into(),
            ));
        }
    }
    let limit = resolve_limit(params.limit)?;
    let cursor = resolve_cursor(params.cursor.as_deref())?;
    let issues = state
        .github_adapter
        .list_issues(&shell_id, state_filter, params.label.as_deref())
        .await
        .map_err(map_port_error)?;
    let (page_items, next) = paginate(&issues, cursor, limit)?;
    Ok(Json(IssuePageResponse {
        items: page_items.iter().map(to_issue_response).collect(),
        next_cursor: next,
    }))
}

/// `POST /projects/{projectId}/issues`（作成、認証必須）。
pub async fn open_issue(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Extension(identity_ext): Extension<RequestIdentity>,
    Json(req): Json<OpenIssueRequest>,
) -> Result<(StatusCode, Json<IssueResponse>), ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let title = req.title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(ApiError::ValidationFailed(
            "タイトルは 1〜200 文字にしてください".into(),
        ));
    }
    if req.body.chars().count() > 10000 {
        return Err(ApiError::ValidationFailed(
            "本文は 10000 文字以下にしてください".into(),
        ));
    }
    if req.labels.len() > 10 {
        return Err(ApiError::ValidationFailed(
            "ラベルは 10 件以下にしてください".into(),
        ));
    }
    let issue = state
        .github_adapter
        .open_issue(&shell_id, title, &req.body, &req.labels)
        .await
        .map_err(map_port_error)?;
    fanout_issue(&state, &identity_ext, &project_id, &shell_id, "作成", &req.title).await?;
    Ok((StatusCode::CREATED, Json(to_issue_response(&issue))))
}

/// `GET /projects/{projectId}/issues/{number}`（詳細、公開）。
pub async fn get_issue(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
) -> Result<Json<IssueResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let issue = state
        .github_adapter
        .get_issue(&shell_id, number)
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_issue_response(&issue)))
}

/// `PATCH /projects/{projectId}/issues/{number}`（更新、認証必須）。
pub async fn update_issue(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
    Json(req): Json<UpdateIssueRequest>,
) -> Result<Json<IssueResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let update = IssueUpdate {
        title: req.title.clone(),
        body: req.body.clone(),
        labels: req.labels.clone(),
        assignee_handle: req.assignee_handle.clone(),
        state: req.state.as_deref().map(parse_issue_state).transpose()?,
    };
    let issue = state
        .github_adapter
        .update_issue(&shell_id, number, &update)
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_issue_response(&issue)))
}

/// `GET /projects/{projectId}/issues/{number}/comments`（コメント一覧、認証必須）。
pub async fn list_issue_comments(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
    Query(params): Query<ListCommentsQuery>,
) -> Result<Json<CommentPageResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let limit = resolve_limit(params.limit)?;
    let cursor = resolve_cursor(params.cursor.as_deref())?;
    let comments = state
        .github_adapter
        .list_issue_comments(&shell_id, number)
        .await
        .map_err(map_port_error)?;
    let (page_items, next) = paginate(&comments, cursor, limit)?;
    Ok(Json(CommentPageResponse {
        items: page_items.iter().map(to_comment_response).collect(),
        next_cursor: next,
    }))
}

/// `POST /projects/{projectId}/issues/{number}/comments`（コメント、認証必須）。
pub async fn comment_issue(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
    Extension(identity_ext): Extension<RequestIdentity>,
    Json(req): Json<CommentIssueRequest>,
) -> Result<(StatusCode, Json<CommentResponse>), ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    if req.body.trim().is_empty() || req.body.chars().count() > 10000 {
        return Err(ApiError::ValidationFailed(
            "コメント本文は 1〜10000 文字にしてください".into(),
        ));
    }
    let comment = state
        .github_adapter
        .comment_issue(&shell_id, number, &req.body)
        .await
        .map_err(map_port_error)?;
    fanout_comment(&state, &identity_ext, &project_id, &shell_id, number, false).await?;
    Ok((StatusCode::CREATED, Json(to_comment_response(&comment))))
}

/// `POST /projects/{projectId}/issues/{number}/close`（クローズ、認証必須）。
pub async fn close_issue(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(String, u64)>,
    Json(req): Json<CloseIssueRequest>,
) -> Result<Json<IssueResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let target = parse_issue_state(&req.state)?;
    let issue = state
        .github_adapter
        .close_issue(&shell_id, number, target)
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_issue_response(&issue)))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    use crate::ports::{Identity, NotificationKind, NotificationPort};
    use crate::test_support::{
        register_test_project, response_text, test_router, test_state_with_github_base, MockGitHub,
    };

    /// mock GitHub + 登録済みプロジェクト + 認証済みセッションを持つ状態を組み立てる。
    async fn setup(prs: Vec<serde_json::Value>) -> (axum::Router, MockGitHub, String) {
        let mock = MockGitHub::new();
        let base_url = mock.start().await;
        mock.set_prs(prs);
        let mut reviews = std::collections::HashMap::new();
        reviews.insert(7, vec![serde_json::json!({"state": "APPROVED"})]);
        mock.set_reviews(reviews);

        let state = test_state_with_github_base(&base_url);
        // GitHub アダプターにテストトークンを設定する（権限エラーを避ける）。
        state
            .github_adapter
            .set_token(crate::adapters::github::client::Token::new("test-token".into()));
        register_test_project(&state, "prj_1", "octo/demo");
        let identity = Identity {
            id: "u-1".into(),
            handle: "sora".into(),
            display_name: "sora".into(),
            avatar_url: None,
        };
        let session = state.sessions.create_with_identity("u-1", Some(identity));
        let token = session.session_id.clone();
        let router = test_router(state);
        (router, mock, token)
    }

    fn open_pr(number: i64) -> serde_json::Value {
        serde_json::json!({
            "number": number,
            "title": format!("T{number}"),
            "body": "B",
            "state": "open",
            "user": {"login": "alice"},
            "base": {"ref": "main"},
            "head": {"ref": "feat"},
            "merged": false,
            "additions": 5,
            "deletions": 2,
            "changed_files": 1,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z"
        })
    }

    fn json_request(
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
        token: Option<&str>,
    ) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(path);
        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        builder
            .body(Body::from(
                body.map(|b| b.to_string()).unwrap_or_default(),
            ))
            .unwrap()
    }

    #[tokio::test]
    async fn list_proposals_route_maps_to_github() {
        let (router, mock, _token) = setup(vec![open_pr(7)]).await;
        let resp = router.clone()
            .oneshot(json_request(Method::GET, "/api/v1/projects/prj_1/proposals", None, None))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        // camelCase フィールド名で契約に準拠。
        assert_eq!(page["items"][0]["authorHandle"].as_str().unwrap(), "alice");
        assert_eq!(page["items"][0]["baseRef"].as_str().unwrap(), "main");
        assert_eq!(page["items"][0]["diffStats"]["additions"].as_u64().unwrap(), 5);
        assert_eq!(page["items"][0]["approvalCount"].as_u64().unwrap(), 1);
        assert!(page["nextCursor"].is_null());
        // ルート→ポート→GitHub の写像: GET /repos/{o}/{r}/pulls が発信される。
        assert!(mock.has("GET", "/repos/octo/demo/pulls"));
    }

    #[tokio::test]
    async fn list_proposals_state_filter_passes_to_github() {
        let (router, mock, _token) = setup(vec![open_pr(7)]).await;
        let resp = router.clone()
            .oneshot(json_request(
                Method::GET,
                "/api/v1/projects/prj_1/proposals?state=open",
                None,
                None,
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        // state=open は GitHub の state=open で取得される。
        let recorded = mock.recorded();
        assert!(recorded
            .iter()
            .any(|r| r.path.contains("/pulls?state=open") || r.path.contains("/pulls&state=open")));
    }

    #[tokio::test]
    async fn open_proposal_requires_auth_and_creates() {
        // 未認証は 401。
        let (router, _mock, _token) = setup(vec![]).await;
        let resp = router.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/proposals",
                Some(serde_json::json!({
                    "title": "T", "headRef": "feat", "baseRef": "main"
                })),
                None,
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // 認証済みは 201 で GitHub の POST /pulls を呼ぶ。
        let (router, mock, token) = setup(vec![]).await;
        let resp = router.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/proposals",
                Some(serde_json::json!({
                    "title": "T", "body": "B", "headRef": "feat", "baseRef": "main"
                })),
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::CREATED, "body={body}");
        let pr: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(pr["number"].as_u64().unwrap(), 7);
        assert_eq!(pr["headRef"].as_str().unwrap(), "feat");
        assert!(mock.has("POST", "/repos/octo/demo/pulls"));
    }

    #[tokio::test]
    async fn get_proposal_and_diff_routes() {
        let (router, mock, _token) = setup(vec![open_pr(7)]).await;
        // 詳細（公開）。
        let resp = router.clone()
            .oneshot(json_request(
                Method::GET,
                "/api/v1/projects/prj_1/proposals/7",
                None,
                None,
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&body).unwrap()["number"].as_u64().unwrap(), 7);
        assert!(mock.has("GET", "/repos/octo/demo/pulls/7"));

        // 差分（公開）。
        let resp = router.clone()
            .oneshot(json_request(
                Method::GET,
                "/api/v1/projects/prj_1/proposals/7/diff",
                None,
                None,
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let diff: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(diff["files"][0]["path"].as_str().unwrap(), "a.md");
        assert_eq!(diff["novnowDiagnosticsDelta"]["introduced"].as_u64().unwrap(), 0);
        assert!(mock.has("GET", "/repos/octo/demo/pulls/7/files"));
    }

    #[tokio::test]
    async fn proposal_comments_routes() {
        let (router, mock, token) = setup(vec![open_pr(7)]).await;
        // コメント一覧（認証必須）。
        let resp = router.clone()
            .oneshot(json_request(
                Method::GET,
                "/api/v1/projects/prj_1/proposals/7/comments",
                None,
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&body).unwrap()["items"].as_array().unwrap().len(), 1);
        assert!(mock.has("GET", "/repos/octo/demo/issues/7/comments"));

        // コメント投稿（認証必須）→ 201。
        let resp = router.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/proposals/7/comments",
                Some(serde_json::json!({"body": "msg"})),
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn review_merge_close_routes() {
        let (router, mock, token) = setup(vec![open_pr(7)]).await;
        // レビュー。
        let resp = router.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/proposals/7/approve",
                Some(serde_json::json!({"verdict": "approve"})),
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert!(mock.has("POST", "/repos/octo/demo/pulls/7/reviews"));

        // マージ。
        let resp = router.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/proposals/7/merge",
                None,
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let mr: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(mr["merged"].as_bool().unwrap());
        assert_eq!(mr["sha"].as_str().unwrap(), "abc123");
        assert!(mock.has("POST", "/repos/octo/demo/pulls/7/merge"));

        // クローズ。
        let resp = router.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/proposals/7/close",
                None,
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert!(mock.has("PATCH", "/repos/octo/demo/pulls/7"));
    }

    #[tokio::test]
    async fn automerge_routes() {
        let (router, mock, token) = setup(vec![open_pr(7)]).await;
        // 状態取得（公開）。
        let resp = router.clone()
            .oneshot(json_request(
                Method::GET,
                "/api/v1/projects/prj_1/proposals/7/automerge",
                None,
                None,
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let am: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(am["state"].as_str().unwrap(), "reserved");
        assert!(mock.has("GET", "/repos/octo/demo/pulls/7"));

        // 予約（認証必須）→ 200。
        let resp = router.clone()
            .oneshot(json_request(
                Method::PUT,
                "/api/v1/projects/prj_1/proposals/7/automerge",
                Some(serde_json::json!({"requiredApprovals": 2, "requiredChecks": ["novnow-verify"]})),
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert!(mock.has("PUT", "/repos/octo/demo/pulls/7/merge"));

        // 取消（認証必須）→ 204。
        let resp = router.clone()
            .oneshot(json_request(
                Method::DELETE,
                "/api/v1/projects/prj_1/proposals/7/automerge",
                None,
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(mock.has("DELETE", "/repos/octo/demo/pulls/7/merge"));
    }

    #[tokio::test]
    async fn issues_routes() {
        let (router, mock, token) = setup(vec![]).await;
        // 一覧（公開）。
        let resp = router.clone()
            .oneshot(json_request(Method::GET, "/api/v1/projects/prj_1/issues", None, None))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&body).unwrap()["items"].as_array().unwrap().len(), 1);
        assert!(mock.has("GET", "/repos/octo/demo/issues"));

        // 作成（認証必須）→ 201。
        let resp = router.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/issues",
                Some(serde_json::json!({"title": "bug", "labels": ["bug"]})),
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::CREATED, "body={body}");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&body).unwrap()["number"].as_u64().unwrap(), 3);
        assert!(mock.has("POST", "/repos/octo/demo/issues"));

        // 詳細（公開）。
        let resp = router.clone()
            .oneshot(json_request(Method::GET, "/api/v1/projects/prj_1/issues/3", None, None))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert!(mock.has("GET", "/repos/octo/demo/issues/3"));

        // 更新（認証必須）。
        let resp = router.clone()
            .oneshot(json_request(
                Method::PATCH,
                "/api/v1/projects/prj_1/issues/3",
                Some(serde_json::json!({"title": "bug2"})),
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert!(mock.has("PATCH", "/repos/octo/demo/issues/3"));

        // コメント一覧（認証必須）。
        let resp = router.clone()
            .oneshot(json_request(
                Method::GET,
                "/api/v1/projects/prj_1/issues/3/comments",
                None,
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert!(mock.has("GET", "/repos/octo/demo/issues/3/comments"));

        // コメント投稿（認証必須）→ 201。
        let resp = router.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/issues/3/comments",
                Some(serde_json::json!({"body": "msg"})),
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::CREATED);

        // クローズ（認証必須）。
        let resp = router.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/issues/3/close",
                Some(serde_json::json!({"state": "closed"})),
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn write_routes_require_auth() {
        // 書き込み系は認証なしで 401。
        let (router, _mock, _token) = setup(vec![]).await;
        let cases = [
            (Method::POST, "/api/v1/projects/prj_1/proposals/7/approve", Some(serde_json::json!({"verdict":"approve"}))),
            (Method::POST, "/api/v1/projects/prj_1/proposals/7/merge", None),
            (Method::POST, "/api/v1/projects/prj_1/proposals/7/close", None),
            (Method::PUT, "/api/v1/projects/prj_1/proposals/7/automerge", Some(serde_json::json!({"requiredApprovals":1}))),
            (Method::DELETE, "/api/v1/projects/prj_1/proposals/7/automerge", None),
            (Method::POST, "/api/v1/projects/prj_1/issues", Some(serde_json::json!({"title":"x"}))),
            (Method::PATCH, "/api/v1/projects/prj_1/issues/3", Some(serde_json::json!({"title":"x"}))),
            (Method::POST, "/api/v1/projects/prj_1/issues/3/close", Some(serde_json::json!({"state":"closed"}))),
        ];
        for (method, path, body) in cases {
            let resp = router
                .clone()
                .oneshot(json_request(method, path, body, None))
                .await
                .unwrap();
            let (status, _) = response_text(resp).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "path={path}");
        }
    }

    #[tokio::test]
    async fn list_pagination_uses_limit_and_cursor() {
        let prs = vec![open_pr(1), open_pr(2), open_pr(3)];
        let (router, _mock, _token) = setup(prs).await;
        // limit=2 → 2 件、nextCursor が残り。
        let resp = router
            .clone()
            .oneshot(json_request(
                Method::GET,
                "/api/v1/projects/prj_1/proposals?limit=2",
                None,
                None,
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 2);
        let cursor = page["nextCursor"].as_str().unwrap().to_string();
        assert!(!cursor.is_empty());

        // cursor で次のページへ。
        let resp = router.clone()
            .oneshot(json_request(
                Method::GET,
                &format!("/api/v1/projects/prj_1/proposals?limit=2&cursor={cursor}"),
                None,
                None,
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        assert!(page["nextCursor"].is_null());
    }

    #[tokio::test]
    async fn merge_proposal_fans_out_notification() {
        let mock = MockGitHub::new();
        let base_url = mock.start().await;
        mock.set_prs(vec![serde_json::json!({
            "number": 7, "title": "T7", "body": "B", "state": "closed",
            "user": {"login": "alice"}, "base": {"ref": "main"}, "head": {"ref": "feat"},
            "merged": true, "merge_commit_sha": "abc123",
            "additions": 5, "deletions": 2, "changed_files": 1,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"
        })]);

        let state = test_state_with_github_base(&base_url);
        state
            .github_adapter
            .set_token(crate::adapters::github::client::Token::new("test-token".into()));
        register_test_project(&state, "prj_1", "octo/demo");
        let notifications = state.notifications.clone();
        let identity = Identity {
            id: "u-1".into(),
            handle: "sora".into(),
            display_name: "sora".into(),
            avatar_url: None,
        };
        let session = state.sessions.create_with_identity("u-1", Some(identity));
        let token = session.session_id.clone();
        let router = test_router(state);

        let resp = router
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_1/proposals/7/merge",
                None,
                Some(&token),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK, "マージが成功する前提");

        let (items, _next) = notifications
            .list_notifications("u-1", None, false, None, 100)
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, NotificationKind::ProposalMerged);
        assert_eq!(items[0].project_id.as_deref(), Some("prj_1"));
        assert!(!items[0].read);
    }

    #[tokio::test]
    async fn unknown_project_returns_404() {
        let (router, _mock, _token) = setup(vec![]).await;
        let resp = router.clone()
            .oneshot(json_request(
                Method::GET,
                "/api/v1/projects/unknown/proposals",
                None,
                None,
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("not_found"));
    }
}
