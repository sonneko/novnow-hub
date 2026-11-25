//! novnow 診断・エクスポートの HTTP ルート（設計 §4.1〜§4.3、契約）。
//!
//! - `POST /projects/{projectId}/diagnostics`：認証必要。診断ジョブを受理し `202` + jobId。
//! - `GET /jobs/{jobId}`：ジョブ状態を取得（`200`）。
//! - `POST /projects/{projectId}/exports`：本文エクスポートジョブを受理し `202` + jobId。
//!
//! GitHub の語彙（GitHub Actions 等）は `JobPort` の背後に隠し、契約・フロントに漏らさない。

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::middleware::RequestIdentity;
use crate::ports::{
    CreateJobRequest, DiagnosticItem, ExportFormat, Job, JobKind, JobResult, PortError,
};

/// `POST /projects/{projectId}/diagnostics` のレスポンス（契約 `JobAccepted`）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobAcceptedResponse {
    pub job_id: String,
    pub status: String,
}

/// `POST /projects/{projectId}/exports` のリクエストボディ（契約 `createExport`）。
#[derive(Debug, Deserialize)]
pub struct ExportBody {
    /// 出力形式（`txt` / `markdown`、既定は `txt`）。
    #[serde(default = "default_export_format")]
    pub format: String,
}

fn default_export_format() -> String {
    "txt".to_string()
}

/// 契約 `Job.result` の JSON 表現（種別ごと）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobResultResponse {
    Diagnostics {
        ok: bool,
        errors: Vec<DiagnosticItemResponse>,
        warnings: Vec<DiagnosticItemResponse>,
        checked_at: String,
    },
    Export {
        format: String,
        char_count: usize,
    },
}

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

/// 契約 `Job` のレスポンス。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobResponse {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub result: Option<JobResultResponse>,
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

impl JobResponse {
    fn from_domain(job: &Job) -> Self {
        JobResponse {
            id: job.id.clone(),
            kind: job.kind.as_str().to_string(),
            // ポートの Cancelled は契約の `failed` に写像する。
            status: job.status.as_contract_str().to_string(),
            result: job.result.as_ref().map(to_result_response),
            error: job.error.clone(),
            created_at: iso8601(job.created_at),
            finished_at: job.finished_at.map(iso8601),
        }
    }
}

fn to_result_response(result: &JobResult) -> JobResultResponse {
    match result {
        JobResult::Diagnostics(d) => JobResultResponse::Diagnostics {
            ok: d.ok,
            errors: d.errors.iter().map(to_diagnostic_item).collect(),
            warnings: d.warnings.iter().map(to_diagnostic_item).collect(),
            checked_at: iso8601(d.checked_at),
        },
        JobResult::Export(e) => JobResultResponse::Export {
            format: e.format.clone(),
            char_count: e.char_count,
        },
    }
}

/// `POST /projects/{projectId}/diagnostics`（契約 `runDiagnostics`、認証必要）。
///
/// プロジェクトが存在しない場合は `404`。ジョブを起票し `202` + jobId を返す。
pub async fn run_diagnostics(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Extension(_identity): Extension<RequestIdentity>,
) -> Result<(StatusCode, Json<JobAcceptedResponse>), ApiError> {
    // プロジェクト存在確認（存在しなければ 404）。
    if state.projects.get(&project_id).is_none() {
        return Err(ApiError::NotFound(
            "指定されたプロジェクトが見つかりません。".to_string(),
        ));
    }

    let job = state
        .jobs
        .create_job(&CreateJobRequest {
            kind: JobKind::Diagnostics,
            project_id: project_id.clone(),
            format: None,
        })
        .await
        .map_err(map_port_error)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(JobAcceptedResponse {
            job_id: job.id,
            status: job.status.as_str().to_string(),
        }),
    ))
}

/// `POST /projects/{projectId}/exports`（契約 `createExport`、無認証）。
///
/// プロジェクトが存在しない場合は `404`。エクスポートジョブを起票し `202` + jobId。
pub async fn create_export(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(body): Json<ExportBody>,
) -> Result<(StatusCode, Json<JobAcceptedResponse>), ApiError> {
    if state.projects.get(&project_id).is_none() {
        return Err(ApiError::NotFound(
            "指定されたプロジェクトが見つかりません。".to_string(),
        ));
    }

    let format = ExportFormat::from_export_format_str(&body.format);
    let job = state
        .jobs
        .create_job(&CreateJobRequest {
            kind: JobKind::Export,
            project_id: project_id.clone(),
            format: Some(format),
        })
        .await
        .map_err(map_port_error)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(JobAcceptedResponse {
            job_id: job.id,
            status: job.status.as_str().to_string(),
        }),
    ))
}

/// `GET /jobs/{jobId}`（契約 `getJob`）。
///
/// ジョブが存在しない場合は `404`。
pub async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobResponse>, ApiError> {
    let job = state
        .jobs
        .get_job(&job_id)
        .await
        .map_err(map_port_error)?;
    Ok(Json(JobResponse::from_domain(&job)))
}

/// ジョブポートのエラーを API エラーへ写像する。
fn map_port_error(e: PortError) -> ApiError {
    match e {
        PortError::NotFound => ApiError::NotFound("指定されたジョブが見つかりません".into()),
        PortError::Conflict => ApiError::Conflict("ジョブの状態が競合しました".into()),
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
    fn export_default_format_is_txt() {
        let body: ExportBody = serde_json::from_str("{}").unwrap();
        assert_eq!(body.format, "txt");
    }

    #[test]
    fn export_accepts_markdown() {
        let body: ExportBody = serde_json::from_str(r#"{"format":"markdown"}"#).unwrap();
        assert_eq!(body.format, "markdown");
    }
}
