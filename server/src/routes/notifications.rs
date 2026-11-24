//! M4 通知センターの HTTP ルーター（設計 §6.1）。
//!
//! 契約 `REF:api/openapi.yaml` の `/notifications` 系エンドポイントに準拠する。
//! 通知は hub 管理ストア（`NotificationPort`）経由でアクセスし、GitHub の
//! 概念は本層に漏らさない。認可は既存の `require_auth` ミドルウェアで認証必須。

use axum::extract::{Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::middleware::RequestIdentity;
use crate::ports::{Notification, NotificationKind, NotificationPort};

const DEFAULT_PAGE_LIMIT: usize = 20;
const MAX_PAGE_LIMIT: usize = 100;

/// 通知のレスポンス型（契約 `Notification`）。
#[derive(Debug, Serialize)]
pub struct NotificationResponse {
    pub id: String,
    pub kind: String,
    pub project_id: Option<String>,
    pub project_slug: Option<String>,
    pub title: String,
    pub body: String,
    pub read: bool,
    pub created_at: String,
}

/// 通知ページのレスポンス型（契約 `NotificationPage`）。
#[derive(Debug, Serialize)]
pub struct NotificationPageResponse {
    pub items: Vec<NotificationResponse>,
    pub next_cursor: Option<String>,
}

/// `GET /notifications` のクエリ。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListNotificationsQuery {
    pub kind: Option<String>,
    pub unread_only: Option<bool>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// `POST /notifications/read` のリクエスト。
#[derive(Debug, Deserialize)]
pub struct MarkReadRequest {
    pub ids: Option<Vec<String>>,
    pub all: Option<bool>,
}

fn to_notification_response(n: &Notification) -> NotificationResponse {
    NotificationResponse {
        id: n.id.clone(),
        kind: n.kind.as_str().to_string(),
        project_id: n.project_id.clone(),
        project_slug: n.project_slug.clone(),
        title: n.title.clone(),
        body: n.body.clone(),
        read: n.read,
        created_at: chrono::DateTime::from_timestamp(n.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_default(),
    }
}

fn parse_kind(raw: &str) -> Result<NotificationKind, ApiError> {
    match raw {
        "derivative_created" => Ok(NotificationKind::DerivativeCreated),
        "proposal_merged" => Ok(NotificationKind::ProposalMerged),
        "proposal_automerge_failed" => Ok(NotificationKind::ProposalAutomergeFailed),
        "proposal_comment" => Ok(NotificationKind::ProposalComment),
        "issue_created" => Ok(NotificationKind::IssueCreated),
        "issue_comment" => Ok(NotificationKind::IssueComment),
        "mention" => Ok(NotificationKind::Mention),
        "job_finished" => Ok(NotificationKind::JobFinished),
        "publish_rebuilt" => Ok(NotificationKind::PublishRebuilt),
        other => Err(ApiError::ValidationFailed(format!(
            "不明な通知種別です: {other}"
        ))),
    }
}

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

fn identity(ext: &RequestIdentity) -> Result<(String, String), ApiError> {
    let user = ext
        .session
        .identity
        .as_ref()
        .ok_or_else(|| ApiError::AuthenticationRequired("利用者の情報がありません".into()))?;
    Ok((user.id.clone(), user.handle.clone()))
}

/// `GET /notifications`（通知一覧、認証必須）。
pub async fn list_notifications(
    State(state): State<AppState>,
    Extension(identity_ext): Extension<RequestIdentity>,
    Query(params): Query<ListNotificationsQuery>,
) -> Result<Response, ApiError> {
    let (user_id, _handle) = identity(&identity_ext)?;
    let limit = resolve_limit(params.limit)?;
    let cursor = resolve_cursor(params.cursor.as_deref())?;
    let kind = params.kind.as_deref().map(parse_kind).transpose()?;

    let (items, next) = state
        .notifications
        .list_notifications(
            &user_id,
            kind,
            params.unread_only.unwrap_or(false),
            Some(&cursor.to_string()),
            limit,
        )
        .await
        .map_err(map_port_error)?;

    let unread = state.notifications.unread_count(&user_id);
    let body = NotificationPageResponse {
        items: items.iter().map(to_notification_response).collect(),
        next_cursor: next,
    };
    let mut response = Json(body).into_response();
    set_unread_header(&mut response, unread);
    Ok(response)
}

/// `POST /notifications/read`（既読化、認証必須）。
pub async fn mark_notifications_read(
    State(state): State<AppState>,
    Extension(identity_ext): Extension<RequestIdentity>,
    Json(req): Json<MarkReadRequest>,
) -> Result<Response, ApiError> {
    let (user_id, _handle) = identity(&identity_ext)?;
    let ids = req.ids.unwrap_or_default();
    if !req.all.unwrap_or(false) && ids.len() > 200 {
        return Err(ApiError::ValidationFailed(
            "一度に既読化できる件数は 200 件以下です".into(),
        ));
    }
    state
        .notifications
        .mark_read(&user_id, &ids, req.all.unwrap_or(false))
        .await
        .map_err(map_port_error)?;

    let unread = state.notifications.unread_count(&user_id);
    let mut response = StatusCode::NO_CONTENT.into_response();
    set_unread_header(&mut response, unread);
    Ok(response)
}

fn set_unread_header(response: &mut Response, unread: usize) {
    if let Ok(value) = HeaderValue::from_str(&unread.to_string()) {
        response
            .headers_mut()
            .insert(header::HeaderName::from_static("x-unread-count"), value);
    }
}

fn map_port_error(e: crate::ports::PortError) -> ApiError {
    match e {
        crate::ports::PortError::NotFound => ApiError::NotFound("対象が見つかりません".into()),
        crate::ports::PortError::Conflict => ApiError::Conflict("競合が発生しました".into()),
        crate::ports::PortError::PermissionDenied => {
            ApiError::PermissionDenied("権限が不足しています".into())
        }
        crate::ports::PortError::RateLimited => ApiError::RateLimited("レートリミットに達しました".into()),
        crate::ports::PortError::Unavailable => {
            ApiError::UpstreamUnavailable("外部基盤が利用できません".into())
        }
        crate::ports::PortError::InvalidResponse => {
            ApiError::Internal("外部基盤の応答が不正です".into())
        }
        crate::ports::PortError::Unknown => ApiError::Internal("不明なエラー".into()),
    }
}
