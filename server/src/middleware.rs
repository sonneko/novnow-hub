//! 認可ミドルウェア。
//!
//! Bearer アクセストークンを検証し、無効なら 401 Problem を返す。
//! 設計書 `REF:docs/designs/api.md` §3.2 に準拠（セッション照合はサーバ側で行う）。
//!
//! 検証したセッションはリクエスト拡張（`RequestIdentity`）として挿入し、
//! ルートがハンドル・利用者情報を得られるようにする（判断:
//! `REF:docs/handoff.md` §4 判断 3）。

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::session::Session;

/// ミドルウェアが挿入する、認証済みのセッション。
#[derive(Debug, Clone)]
pub struct RequestIdentity {
    /// 認証済みのセッション。
    pub session: Session,
}

/// 認証を要求するミドルウェア。
pub async fn require_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let headers = request.headers();
    let value = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::AuthenticationRequired("認証ヘッダーがありません".into()))?;
    let session_id = value
        .strip_prefix("Bearer ")
        .filter(|t| !t.is_empty())
        .ok_or_else(|| ApiError::AuthenticationRequired("Bearer トークンが無効です".into()))?;
    let session = state
        .sessions
        .verify_access(session_id)
        .ok_or_else(|| ApiError::AuthenticationRequired("セッションが無効です".into()))?;

    // セッション（と紐付いた Identity）をリクエスト拡張へ挿入する。
    let mut request = request;
    request.extensions_mut().insert(RequestIdentity { session });
    Ok(next.run(request).await)
}
