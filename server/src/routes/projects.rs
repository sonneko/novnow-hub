//! 公開フローのルーター。
//!
//! `POST /projects` を実装する。仕様 `REF:docs/specifications/m3.md` §2、
//! 設計 `REF:docs/designs/m3.md` §2.1・§2.2、契約 `REF:api/openapi.yaml`
//! の `POST /projects` に準拠する。
//!
//! 処理の流れ（設計 §2.1）:
//! multipart 受領 → zip 取込（許可リスト検査）→ novnow 軽量検証 →
//! 保管単位の作成（ポート経由）→ ファイルコミット → hub 登録 →
//! 公開ページ URL 生成。
//!
//! いずれかの工程で失敗したら中間状態（保管単位）を残さずエラーを返す。
//! GitHub の概念は本層に漏らさず、すべてポート経由（`ProjectStoragePort`・
//! `ContentPort`）でアクセスする。

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::middleware::RequestIdentity;
use crate::multipart::{self, Part};
use crate::ports::{ContentPort, FileEncoding, NewFile, ProjectStoragePort, ProjectVisibility};
use crate::registry::{self, new_project_id, now_unix, slugify, shell_name};
use crate::verify::{self, SnapshotFile};
use crate::zip_import;

/// multipart の上限（設計 §10.3 の zip 取込上限と一致）。
const MAX_UPLOAD_BYTES: usize = zip_import::MAX_UPLOAD_BYTES;

/// ライセンスの列挙（契約 `license` の値）。
fn parse_license(raw: &str) -> Result<String, ApiError> {
    match raw.trim() {
        "cc0" | "cc-by" | "cc-by-sa" | "cc-by-nc" | "custom" => Ok(raw.trim().to_string()),
        other => Err(ApiError::ValidationFailed(format!(
            "ライセンスが不正です: {other}"
        ))),
    }
}

/// 公開範囲の列挙（契約 `visibility` の値。既定は public）。
fn parse_visibility(raw: &str) -> Result<ProjectVisibility, ApiError> {
    match raw.trim() {
        "" | "public" => Ok(ProjectVisibility::Public),
        "private" => Ok(ProjectVisibility::Private),
        "archived" => Ok(ProjectVisibility::Archived),
        other => Err(ApiError::ValidationFailed(format!(
            "公開範囲が不正です: {other}"
        ))),
    }
}

/// 公開ページ URL（`publishedUrl`）。
///
/// 判断（`REF:docs/handoff.md` §4 判断 1）により
/// `{HUB_PUBLIC_BASE}/reading/?projectId={projectId}` を機械生成する。
fn published_url(config: &crate::config::Config, project_id: &str) -> String {
    let base = config.hub_public_base.trim_end_matches('/');
    format!("{base}/reading/?projectId={project_id}")
}

/// セットアップ URL。
fn setup_url(config: &crate::config::Config, project_id: &str) -> String {
    let base = config.hub_public_base.trim_end_matches('/');
    let u = format!("{base}/projects/{project_id}/setup");
    format!("{u}?u={}", urlencoding(&u))
}

/// URL クエリ値として安全な文字列へのエンコード。
fn urlencoding(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

/// 公開完了レスポンス（契約の `Project` + セットアップ URL）。
///
/// 契約 §2.2 の `Project` スキーマに必須フィールドを揃え、追加として
/// `setupUrl` を含む（判断: 設計 §2.2）。
#[derive(Debug, Serialize)]
pub struct PublishResponse {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub owner_handle: String,
    pub license: String,
    pub visibility: String,
    pub derivative_policy: String,
    pub derivative_count: i64,
    pub default_branch_name: String,
    pub etag: String,
    pub created_at: String,
    pub updated_at: String,
    pub published_url: String,
    /// セットアップ URL（設計 §2.2）。
    pub setup_url: String,
}

/// `POST /projects`。
///
/// `require_auth` 経由で呼ばれ、リクエスト拡張に認証済みセッションが挿入される。
pub async fn create_project(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, ApiError> {
    // 認証済みのセッションから利用者情報を取り出す。
    let identity_ext = request
        .extensions()
        .get::<RequestIdentity>()
        .cloned()
        .ok_or_else(|| ApiError::AuthenticationRequired("認証情報がありません".into()))?;
    let identity = identity_ext
        .session
        .identity
        .ok_or_else(|| ApiError::AuthenticationRequired("利用者の情報がありません".into()))?;
    // 後続で借用と移動が混在するため、ハンドルは先に確保しておく。
    let owner_handle = identity.handle.clone();

    // Content-Type とボディを取り出す。
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::ValidationFailed("Content-Type がありません".into()))?
        .to_string();

    let body = read_body(request)
        .await
        .map_err(ApiError::ValidationFailed)?;

    if body.len() > MAX_UPLOAD_BYTES {
        return Err(ApiError::PayloadTooLarge(
            "アップロードが大きすぎます".into(),
        ));
    }

    // multipart パース。
    let parts = multipart::parse(&content_type, &body)
        .map_err(|e| ApiError::ValidationFailed(e.to_string()))?;

    let field = |name: &str| -> Result<Part, ApiError> {
        parts
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .ok_or_else(|| ApiError::ValidationFailed(format!("必須フィールドが欠けています: {name}")))
    };

    let title_part = field("title")?;
    let license_part = field("license")?;
    let archive_part = field("archive")?;

    let title = title_part.text().trim().to_string();
    if title.is_empty() || title.chars().count() > 120 {
        return Err(ApiError::ValidationFailed(
            "タイトルは 1〜120 文字にしてください".into(),
        ));
    }
    let description_part = parts.iter().find(|p| p.name == "description");
    let description = description_part
        .map(|p| p.text().trim().to_string())
        .filter(|d| !d.is_empty());
    if let Some(d) = &description {
        if d.chars().count() > 500 {
            return Err(ApiError::ValidationFailed(
                "説明は 500 文字以下にしてください".into(),
            ));
        }
    }
    let license = parse_license(&license_part.text())?;
    let visibility = match parts.iter().find(|p| p.name == "visibility") {
        Some(p) => parse_visibility(&p.text())?,
        None => ProjectVisibility::Public,
    };

    // zip 取込（許可リスト検査を含む）。
    let zip_bytes = &archive_part.content;
    let imported = zip_import::import_zip(zip_bytes)
        .map_err(|e| map_zip_error(e, zip_bytes.len()))?;

    // novnow 軽量検証（保管基盤への投入前）。
    let snapshot: Vec<SnapshotFile> = imported
        .iter()
        .map(|f| SnapshotFile {
            path: f.path.clone(),
            content: f.content.clone(),
        })
        .collect();
    if let Err(verr) = verify::verify_snapshot(&snapshot) {
        return Err(ApiError::VerificationFailed(verr.to_string()));
    }

    // 保管単位の作成（ポート経由）。失敗時は登録簿に何も書かない。
    let slug = slugify(&title);
    let shell_name = shell_name(&owner_handle, &slug);
    let shell = state
        .github_adapter
        .create_project_shell(&shell_name, description.as_deref())
        .await
        .map_err(map_port_error)?;

    // ファイルのコミット（保管単位へ投入）。
    let new_files: Vec<NewFile> = imported
        .iter()
        .map(|f| {
            let content = String::from_utf8_lossy(&f.content).into_owned();
            NewFile {
                path: f.path.clone(),
                content,
                encoding: FileEncoding::Text,
            }
        })
        .collect();
    state
        .github_adapter
        .commit_files(&shell.id, &shell.default_branch, &new_files)
        .await
        .map_err(map_port_error)?;

    // 公開範囲の設定。
    if visibility != shell.visibility {
        state
            .github_adapter
            .set_visibility(&shell.id, visibility)
            .await
            .map_err(map_port_error)?;
    }

    // hub 登録（インメモリ登録簿）。
    let project_id = new_project_id();
    let now = now_unix();
    let record = registry::ProjectRecord {
        project_id: project_id.clone(),
        title: title.clone(),
        description: description.clone(),
        owner_handle: owner_handle.clone(),
        license: license.clone(),
        visibility,
        shell_id: shell.id.clone(),
        default_branch: shell.default_branch.clone(),
        etag: shell.etag.clone(),
        created_at: now,
        updated_at: now,
    };
    state.projects.register(record);

    // 公開完了レスポンス（設計 §2.2）。
    let resp = PublishResponse {
        id: project_id.clone(),
        slug: slug.clone(),
        title,
        description,
        owner_handle: owner_handle.clone(),
        license,
        visibility: registry::visibility_str(visibility).to_string(),
        derivative_policy: "allowed".to_string(),
        derivative_count: 0,
        default_branch_name: shell.default_branch.clone(),
        etag: shell.etag.clone(),
        created_at: iso8601(now),
        updated_at: iso8601(now),
        published_url: published_url(&state.config, &project_id),
        setup_url: setup_url(&state.config, &project_id),
    };

    Ok((StatusCode::CREATED, Json(resp)).into_response())
}

/// HTTP ボディを読み出す。
async fn read_body(request: Request) -> Result<Vec<u8>, String> {
    let body = request.into_body();
    let bytes = axum::body::to_bytes(body, MAX_UPLOAD_BYTES + 1)
        .await
        .map_err(|e| format!("ボディの読み取りに失敗: {e}"))?;
    // 上限超過は明示エラー。
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("アップロードが大きすぎます".into());
    }
    Ok(bytes.to_vec())
}

/// UNIX 秒を ISO8601（UTC）へ変換。
fn iso8601(unix: i64) -> String {
    // chrono が利用可能（Cargo.toml に依存）。
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// zip 取込エラーを API エラーへ写像する。
fn map_zip_error(e: zip_import::ZipError, _size: usize) -> ApiError {
    match e {
        zip_import::ZipError::TooLarge => ApiError::PayloadTooLarge("アップロードが大きすぎます".into()),
        zip_import::ZipError::TooManyFiles
        | zip_import::ZipError::CompressionRatioExceeded
        | zip_import::ZipError::Unsupported(_)
        | zip_import::ZipError::PathTraversal(_)
        | zip_import::ZipError::InvalidFilename(_)
        | zip_import::ZipError::NotUtf8(_) => {
            ApiError::ValidationFailed(format!("zip 取込の許可リスト検査に失敗しました: {e}"))
        }
        zip_import::ZipError::InvalidArchive(_)
        | zip_import::ZipError::Inflate(_)
        | zip_import::ZipError::CrcMismatch(_) => {
            ApiError::ValidationFailed(format!("zip が不正です: {e}"))
        }
    }
}

/// ポートエラーを API エラーへ写像する。
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
        crate::ports::PortError::InvalidResponse => ApiError::Internal("外部基盤の応答が不正です".into()),
        crate::ports::PortError::Unknown => ApiError::Internal("不明なエラー".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_url_uses_public_base() {
        let config = crate::config::Config {
            contract_version: "1.0.0".into(),
            minimum_client_version: "1.0.0".into(),
            github_oauth_client_id: None,
            github_oauth_client_secret: None,
            github_oauth_redirect_uri: None,
            github_api_base: "https://api.github.com".into(),
            github_auth_base: "https://github.com".into(),
            access_token_ttl_secs: 3600,
            refresh_token_ttl_secs: 30 * 24 * 3600,
            bind_addr: "127.0.0.1".into(),
            read_timeout_secs: 5,
            write_timeout_secs: 30,
            commit_timeout_secs: 120,
            port: 8080,
            hub_public_base: "http://localhost:4321".into(),
        };
        assert_eq!(
            published_url(&config, "prj_1"),
            "http://localhost:4321/reading/?projectId=prj_1"
        );
    }

    #[test]
    fn license_and_visibility_parse() {
        assert_eq!(parse_license("cc-by").unwrap(), "cc-by");
        assert!(parse_license("unknown").is_err());
        assert_eq!(parse_visibility("").unwrap(), ProjectVisibility::Public);
        assert_eq!(parse_visibility("private").unwrap(), ProjectVisibility::Private);
        assert!(parse_visibility("bogus").is_err());
    }

    #[test]
    fn default_branch_is_main() {
        assert_eq!(crate::registry::DEFAULT_BRANCH, "main");
    }
}
