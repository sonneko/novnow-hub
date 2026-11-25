//! 派生作品の HTTP ルート（設計 §2.2、契約 `listDerivatives`/`deriveProject`）。
//!
//! - `GET /projects/{projectId}/derivatives`：公開。派生ツリー（全深度）を返す。
//! - `POST /projects/{projectId}/derive`：認証必要。派生作成を受理し `202` + jobId を返す。
//!
//! GitHub の語彙（Fork 等）は `DerivationPort` の背後に隠し、契約・フロントに漏らさない。

use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde::Serialize;

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::middleware::RequestIdentity;
use crate::ports::{DeriveAccepted, DeriveRequest, Derivative, JobStatus};

/// `GET /projects/{projectId}/derivatives` のレスポンス（契約 `DerivativeNode`）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivativeNodeResponse {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub owner_handle: String,
    pub depth: u32,
    pub parent_id: Option<String>,
    pub derivative_policy: String,
    pub created_at: String,
    pub updated_at: String,
}

impl DerivativeNodeResponse {
    fn from_domain(d: &Derivative) -> Self {
        DerivativeNodeResponse {
            id: d.id.clone(),
            slug: d.slug.clone(),
            title: d.title.clone(),
            owner_handle: d.owner_handle.clone(),
            depth: d.depth,
            parent_id: d.parent_id.clone(),
            derivative_policy: d.derivative_policy.as_str().to_string(),
            created_at: iso8601(d.created_at),
            updated_at: iso8601(d.updated_at),
        }
    }
}

/// 派生作成リクエスト（契約 `deriveProject` のリクエストボディ）。
#[derive(Debug, Deserialize)]
pub struct DeriveRequestBody {
    /// 派生作品の題名（必須）。
    pub title: String,
    /// 説明の初期値（任意）。
    #[serde(default)]
    pub description: Option<String>,
    /// 公開範囲（`public` / `private`、既定は `private`）。
    #[serde(default)]
    pub visibility: Option<String>,
}

/// `POST /projects/{projectId}/derive` のレスポンス（契約 `JobAccepted`）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobAcceptedResponse {
    pub job_id: String,
    pub status: String,
}

/// `GET /projects/{projectId}/derivatives`（契約 `listDerivatives`、公開）。
pub async fn list(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<DerivativeNodeResponse>>, ApiError> {
    // プロジェクトの存在確認（無ければ 404）。
    let parent = state
        .projects
        .get(&project_id)
        .ok_or_else(|| ApiError::NotFound(format!("プロジェクトが見つかりません: {project_id}")))?;

    // 派生元連鎖 + 子孫の木を取得し、ノードの集合として返す。
    let tree = state
        .derivation
        .get_derivation_tree(&project_id)
        .await
        .map_err(map_port_error)?;
    let _ = parent;
    Ok(Json(
        tree.into_iter()
            .map(|d| DerivativeNodeResponse::from_domain(&d))
            .collect(),
    ))
}

/// `POST /projects/{projectId}/derive`（契約 `deriveProject`、認証必要・Idempotency-Key）。
pub async fn derive(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Extension(identity_ext): Extension<RequestIdentity>,
    headers: HeaderMap,
    Json(body): Json<DeriveRequestBody>,
) -> Result<(StatusCode, Json<JobAcceptedResponse>), ApiError> {
    let identity = identity_ext
        .session
        .identity
        .as_ref()
        .ok_or_else(|| ApiError::AuthenticationRequired("利用者の情報がありません".into()))?;

    // Idempotency-Key に対応：同一キーは既受理のジョブ ID を返す（冪等）。
    let key = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(existing) = key
        .as_deref()
        .and_then(|k| state.derivatives.job_for_idempotency_key(k))
    {
        return Ok((
            StatusCode::ACCEPTED,
            Json(JobAcceptedResponse {
                job_id: existing,
                status: JobStatus::Pending.as_str().to_string(),
            }),
        ));
    }

    let visibility = match body.visibility.as_deref().unwrap_or("private") {
        "public" => crate::ports::ProjectVisibility::Public,
        _ => crate::ports::ProjectVisibility::Private,
    };
    let req = DeriveRequest {
        title: body.title.clone(),
        description: body.description.clone(),
        visibility,
    };

    let accepted: DeriveAccepted = state
        .derivation
        .create_derivative(
            &project_id,
            &identity.id,
            &identity.handle,
            &req,
        )
        .await
        .map_err(map_port_error)?;

    if let Some(k) = key.as_deref() {
        state
            .derivatives
            .remember_idempotency_key(k, &accepted.job_id);
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(JobAcceptedResponse {
            job_id: accepted.job_id,
            status: accepted.status.as_str().to_string(),
        }),
    ))
}

/// 派生ポートのエラーを API エラーへ写像する。
fn map_port_error(e: crate::ports::PortError) -> ApiError {
    match e {
        crate::ports::PortError::NotFound => ApiError::NotFound("対象が見つかりません".into()),
        crate::ports::PortError::Conflict => {
            ApiError::Conflict("派生が許可されていない作品です".into())
        }
        crate::ports::PortError::PermissionDenied => {
            ApiError::PermissionDenied("権限が不足しています".into())
        }
        crate::ports::PortError::RateLimited => ApiError::RateLimited("レートリミットに達しました".into()),
        crate::ports::PortError::Unavailable => {
            ApiError::UpstreamUnavailable("外部基盤が利用できません".into())
        }
        crate::ports::PortError::InvalidResponse => ApiError::Internal("外部基盤の応答が不正です".into()),
        crate::ports::PortError::Unknown => ApiError::Internal("不明なエラー".into()),
    }
}

/// UNIX 秒を ISO 8601（UTC）へ変換する。
fn iso8601(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}
