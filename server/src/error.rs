//! RFC 9457 に準拠する Problem Details 応答。
//!
//! 契約 `REF:api/openapi.yaml` の `components/schemas/Problem` に準拠する。
//! `code` は契約の列挙値のみを使う。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// 契约の `code` 列挙（`Problem.code`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemCode {
    ValidationFailed,
    AuthenticationRequired,
    SessionExpired,
    RefreshTokenReused,
    PermissionDenied,
    NotFound,
    Conflict,
    IdempotencyMismatch,
    PayloadTooLarge,
    RateLimited,
    UpstreamUnavailable,
    VerificationFailed,
    JobNotFound,
    InternalError,
}

impl ProblemCode {
    fn as_str(self) -> &'static str {
        match self {
            ProblemCode::ValidationFailed => "validation_failed",
            ProblemCode::AuthenticationRequired => "authentication_required",
            ProblemCode::SessionExpired => "session_expired",
            ProblemCode::RefreshTokenReused => "refresh_token_reused",
            ProblemCode::PermissionDenied => "permission_denied",
            ProblemCode::NotFound => "not_found",
            ProblemCode::Conflict => "conflict",
            ProblemCode::IdempotencyMismatch => "idempotency_mismatch",
            ProblemCode::PayloadTooLarge => "payload_too_large",
            ProblemCode::RateLimited => "rate_limited",
            ProblemCode::UpstreamUnavailable => "upstream_unavailable",
            ProblemCode::VerificationFailed => "verification_failed",
            ProblemCode::JobNotFound => "job_not_found",
            ProblemCode::InternalError => "internal_error",
        }
    }

    fn title(self) -> &'static str {
        match self {
            ProblemCode::ValidationFailed => "入力値が契約に違反しています",
            ProblemCode::AuthenticationRequired => "認証が必要です",
            ProblemCode::SessionExpired => "セッションの有効期限が切れました",
            ProblemCode::RefreshTokenReused => "セッションが無効になりました",
            ProblemCode::PermissionDenied => "権限が不足しています",
            ProblemCode::NotFound => "対象が見つかりません",
            ProblemCode::Conflict => "競合が発生しました",
            ProblemCode::IdempotencyMismatch => "冪等性キーが一致しません",
            ProblemCode::PayloadTooLarge => "リクエストが大きすぎます",
            ProblemCode::RateLimited => "レートリミットに達しました",
            ProblemCode::UpstreamUnavailable => "外部基盤が利用できません",
            ProblemCode::VerificationFailed => "検証に失敗しました",
            ProblemCode::JobNotFound => "ジョブが見つかりません",
            ProblemCode::InternalError => "内部エラーが発生しました",
        }
    }

    fn status(self) -> StatusCode {
        match self {
            ProblemCode::ValidationFailed => StatusCode::BAD_REQUEST,
            ProblemCode::AuthenticationRequired
            | ProblemCode::SessionExpired
            | ProblemCode::RefreshTokenReused => StatusCode::UNAUTHORIZED,
            ProblemCode::PermissionDenied => StatusCode::FORBIDDEN,
            ProblemCode::NotFound | ProblemCode::JobNotFound => StatusCode::NOT_FOUND,
            ProblemCode::Conflict => StatusCode::CONFLICT,
            ProblemCode::IdempotencyMismatch => StatusCode::BAD_REQUEST,
            ProblemCode::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ProblemCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ProblemCode::UpstreamUnavailable => StatusCode::BAD_GATEWAY,
            ProblemCode::VerificationFailed => StatusCode::BAD_REQUEST,
            ProblemCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Problem Details のボディ。
#[derive(Debug, Serialize)]
pub struct Problem {
    #[serde(rename = "type")]
    pub type_uri: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
    pub trace_id: String,
}

/// バックエンドのエラー型。`Problem` に変換して応答する。
#[derive(Debug)]
pub enum ApiError {
    ValidationFailed(String),
    AuthenticationRequired(String),
    SessionExpired(String),
    RefreshTokenReused(String),
    PermissionDenied(String),
    NotFound(String),
    Conflict(String),
    PayloadTooLarge(String),
    RateLimited(String),
    UpstreamUnavailable(String),
    VerificationFailed(String),
    Internal(String),
}

impl ApiError {
    pub fn problem(&self) -> (ProblemCode, String) {
        match self {
            ApiError::ValidationFailed(d) => (ProblemCode::ValidationFailed, d.clone()),
            ApiError::AuthenticationRequired(d) => (ProblemCode::AuthenticationRequired, d.clone()),
            ApiError::SessionExpired(d) => (ProblemCode::SessionExpired, d.clone()),
            ApiError::RefreshTokenReused(d) => (ProblemCode::RefreshTokenReused, d.clone()),
            ApiError::PermissionDenied(d) => (ProblemCode::PermissionDenied, d.clone()),
            ApiError::NotFound(d) => (ProblemCode::NotFound, d.clone()),
            ApiError::Conflict(d) => (ProblemCode::Conflict, d.clone()),
            ApiError::PayloadTooLarge(d) => (ProblemCode::PayloadTooLarge, d.clone()),
            ApiError::RateLimited(d) => (ProblemCode::RateLimited, d.clone()),
            ApiError::UpstreamUnavailable(d) => (ProblemCode::UpstreamUnavailable, d.clone()),
            ApiError::VerificationFailed(d) => (ProblemCode::VerificationFailed, d.clone()),
            ApiError::Internal(d) => (ProblemCode::InternalError, d.clone()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, detail) = self.problem();
        let problem = Problem {
            type_uri: format!("https://novnow.hub/problems/{}", code.as_str()),
            title: code.title().to_string(),
            status: code.status().as_u16(),
            detail: Some(detail),
            code: code.as_str().to_string(),
            retry_after_seconds: None,
            trace_id: generate_trace_id(),
        };
        (code.status(), Json(problem)).into_response()
    }
}

/// 追跡 ID（簡易的 ULID 風の不透明文字列）。
fn generate_trace_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_maps_each_variant_to_code() {
        let cases: Vec<(ApiError, ProblemCode)> = vec![
            (ApiError::ValidationFailed("x".into()), ProblemCode::ValidationFailed),
            (
                ApiError::AuthenticationRequired("x".into()),
                ProblemCode::AuthenticationRequired,
            ),
            (ApiError::SessionExpired("x".into()), ProblemCode::SessionExpired),
            (
                ApiError::RefreshTokenReused("x".into()),
                ProblemCode::RefreshTokenReused,
            ),
            (ApiError::PermissionDenied("x".into()), ProblemCode::PermissionDenied),
            (ApiError::NotFound("x".into()), ProblemCode::NotFound),
            (ApiError::Conflict("x".into()), ProblemCode::Conflict),
            (ApiError::RateLimited("x".into()), ProblemCode::RateLimited),
            (
                ApiError::UpstreamUnavailable("x".into()),
                ProblemCode::UpstreamUnavailable,
            ),
            (
                ApiError::VerificationFailed("x".into()),
                ProblemCode::VerificationFailed,
            ),
            (ApiError::Internal("x".into()), ProblemCode::InternalError),
        ];
        for (err, expected) in cases {
            let (code, _) = err.problem();
            assert_eq!(code, expected);
        }
    }
}
