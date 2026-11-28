//! リリース（版）の作成（契約 `createRelease`）。
//!
//! 契約 `api/openapi.yaml` の `/projects/{projectId}/releases` は **`post`
//! のみ**を定義する（一覧取得の `get` は無い）。本ルーターも `post` のみを
//! 実装し、契約に無い `GET` は追加しない（判断:
//! `REF:docs/handoff.md` §4 判断 28）。
//!
//! - 認証必須（`security` は既定の `bearerAuth`）。所有者のみ作成できる。
//! - 未登録・所有者以外・権限不足は契約どおり `404`／`403` で返す。
//! - 同じタグで同名・同説明の再送は冪等に同じレコードを返し `201` とする。

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::middleware::RequestIdentity;
use crate::releases::ReleaseError;

use super::shared::{iso8601, resolve_project};

/// リリース作成のリクエストボディ（契約 `createRelease`）。
#[derive(Debug, Deserialize)]
pub struct CreateReleaseBody {
    /// 版の識別子。`^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$`。
    pub tag: String,
    /// 版の名称（1〜120 文字）。
    pub name: String,
    /// 版の説明（最大 10000 文字）。
    #[serde(default)]
    pub notes: Option<String>,
}

/// リリース（契約 `Release`）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseResponse {
    pub tag: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub created_at: String,
}

/// `tag` が契約の正規表現に合うか検証する。
///
/// 契約のパターン `^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$` を
/// 正規表現 crate を使わずに検査する（依存追加は不可）。
fn is_valid_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else {
        return false;
    };
    let (core, prerelease) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    if !parts
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    if let Some(pre) = prerelease {
        if pre.is_empty() {
            return false;
        }
        if !pre
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        {
            return false;
        }
    }
    true
}

/// `POST /projects/{projectId}/releases`（契約 `createRelease`、認証必須）。
pub async fn create_release(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Extension(identity): Extension<RequestIdentity>,
    Json(body): Json<CreateReleaseBody>,
) -> Result<Response, ApiError> {
    // 認証済みの利用者ハンドル（所有者判定に使う）。
    let handle = identity
        .session
        .identity
        .as_ref()
        .map(|i| i.handle.clone())
        .ok_or_else(|| ApiError::AuthenticationRequired("利用者の情報がありません".into()))?;

    let record = resolve_project(&state, &project_id)?;
    if !record.owner_handle.eq_ignore_ascii_case(&handle) {
        return Err(ApiError::PermissionDenied(
            "このプロジェクトのリリースを作成する権限がありません".to_string(),
        ));
    }

    let tag = body.tag.trim().to_string();
    if !is_valid_tag(&tag) {
        return Err(ApiError::ValidationFailed(format!(
            "tag は v{{major}}.{{minor}}.{{patch}} 形式にしてください: {tag}"
        )));
    }
    let name = body.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(ApiError::ValidationFailed(
            "name は 1〜120 文字にしてください".to_string(),
        ));
    }
    let notes = body
        .notes
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    if let Some(notes) = &notes {
        if notes.chars().count() > 10_000 {
            return Err(ApiError::ValidationFailed(
                "notes は 10000 文字以下にしてください".to_string(),
            ));
        }
    }

    let release = state
        .releases
        .create(&project_id, &tag, &name, notes.as_deref())
        .map_err(|e| match e {
            ReleaseError::DuplicateTag => ApiError::ValidationFailed(format!(
                "同じ tag のリリースが別の内容で既に存在します: {tag}"
            )),
        })?;

    let resp = ReleaseResponse {
        tag: release.tag,
        name: release.name,
        notes: release.notes,
        created_at: iso8601(release.created_at),
    };
    Ok((StatusCode::CREATED, Json(resp)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_semver_tags() {
        for tag in [
            "v1.0.0",
            "v0.1.0",
            "v10.20.30",
            "v1.2.3-rc.1",
            "v1.2.3-alpha-2",
        ] {
            assert!(is_valid_tag(tag), "{tag} は受理されるべき");
        }
    }

    #[test]
    fn rejects_non_semver_tags() {
        for tag in [
            "",
            "1.0.0",
            "v1.0",
            "v1.0.0.0",
            "v1..0",
            "v-1.0.0",
            "v1.0.0-",
            "v1.0.x",
            "v1.0.0_beta",
        ] {
            assert!(!is_valid_tag(tag), "{tag} は拒否されるべき");
        }
    }
}
