//! 公開前検証（novnow 点検）の HTTP ルート（設計 §6.1、契約）。
//!
//! - `POST /projects/{projectId}/verify`：認証必要。novnow 軽量検証を同期で
//!   実行し `200` + `VerificationResult` を返す。結果はサービス側に保持する。
//! - `GET /projects/{projectId}/verify/latest`：匿名可。直近の検証結果を
//!   返す（未検証は `404`）。
//!
//! GitHub の概念は `VerificationService`（`JobContentPort` 背後に隠す）の
//! 背後に隠し、契約・フロントに漏らさない。

use axum::extract::{Extension, Path, State};
use axum::Json;
use serde::Serialize;

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::middleware::RequestIdentity;
use crate::ports::{DiagnosticItem, DiagnosticsResult, PortError};

/// 契約 `VerificationResult` の要素。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticItemResponse {
    pub path: String,
    pub code: String,
    pub message: String,
}

fn to_diagnostic_item(item: &DiagnosticItem) -> DiagnosticItemResponse {
    DiagnosticItemResponse {
        path: item.path.clone(),
        code: item.code.clone(),
        message: item.message.clone(),
    }
}

/// 契約 `VerificationResult` のレスポンス。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationResultResponse {
    pub ok: bool,
    pub errors: Vec<DiagnosticItemResponse>,
    pub warnings: Vec<DiagnosticItemResponse>,
    pub checked_at: String,
}

fn to_verification_response(result: &DiagnosticsResult) -> VerificationResultResponse {
    VerificationResultResponse {
        ok: result.ok,
        errors: result.errors.iter().map(to_diagnostic_item).collect(),
        warnings: result.warnings.iter().map(to_diagnostic_item).collect(),
        checked_at: iso8601(result.checked_at),
    }
}

/// `POST /projects/{projectId}/verify`（契約 `verifyProject`、認証必要）。
///
/// プロジェクトが存在しない場合は `404`。novnow 軽量検証を同期で実行し、
/// `200` + `VerificationResult` を返す。
pub async fn verify_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Extension(_identity): Extension<RequestIdentity>,
) -> Result<Json<VerificationResultResponse>, ApiError> {
    // プロジェクト存在確認（存在しなければ 404）。
    if state.projects.get(&project_id).is_none() {
        return Err(ApiError::NotFound(
            "指定されたプロジェクトが見つかりません。".to_string(),
        ));
    }
    let result = state
        .verification
        .verify(&project_id)
        .await
        .map_err(map_port_error)?;
    Ok(Json(to_verification_response(&result)))
}

/// `GET /projects/{projectId}/verify/latest`（契約 `getLatestVerification`、匿名可）。
///
/// プロジェクトが存在しない場合は `404`。未検証（直近結果が無い）場合も
/// `404` を返す（契約の「未実行の場合は 404」）。
pub async fn get_latest_verification(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<VerificationResultResponse>, ApiError> {
    if state.projects.get(&project_id).is_none() {
        return Err(ApiError::NotFound(
            "指定されたプロジェクトが見つかりません。".to_string(),
        ));
    }
    let result = state
        .verification
        .latest(&project_id)
        .ok_or_else(|| ApiError::NotFound("検証結果がまだありません".to_string()))?;
    Ok(Json(to_verification_response(&result)))
}

/// ポートエラーを API エラーへ写像する。
fn map_port_error(e: PortError) -> ApiError {
    match e {
        PortError::NotFound => ApiError::NotFound("指定されたプロジェクトが見つかりません".into()),
        PortError::Conflict => ApiError::Conflict("競合が発生しました".into()),
        PortError::PermissionDenied => ApiError::PermissionDenied("権限が不足しています".into()),
        PortError::RateLimited => ApiError::RateLimited("レートリミットに達しました".into()),
        PortError::Unavailable => ApiError::UpstreamUnavailable("外部基盤が利用できません".into()),
        PortError::InvalidResponse => ApiError::Internal("外部基盤の応答が不正です".into()),
        PortError::Unknown => ApiError::Internal("不明なエラー".into()),
    }
}

fn iso8601(unix: i64) -> String {
    use chrono::DateTime;
    DateTime::<chrono::Utc>::from_timestamp(unix, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_formats_epoch() {
        // 2026-01-01T00:00:00Z 相当。
        let ts = 1767225600;
        let s = iso8601(ts);
        assert!(s.starts_with("2026-01-01"));
    }
}
