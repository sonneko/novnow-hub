//! セットアップ URL の発行・解決（契約 `getSetupInfo`／`resolveSetupUrl`）。
//!
//! - `GET /projects/{projectId}/setup`：このプロジェクト用のセットアップ情報
//!   （`SetupInfo`）を返す。契約で `security: []` のため匿名可。
//! - `GET /setup/resolve?u=…`：セットアップ URL（公開ページの URL 形式）を
//!   解釈し、同じ `SetupInfo` を返す。契約で `security: []` のため匿名可。
//!
//! 応答には保管基盤（GitHub）の語彙を一切含めない。`acquisition.url` は
//! 認証不要の公開アーカイブ URL とし、hub のセッションをアプリへ渡さない
//! （`REF:docs/designs/m5.md` §3.1・§3.2）。
//!
//! 判断（`REF:docs/handoff.md` §4 判断 28）:
//! - `slug` は登録簿のタイトルから機械生成（`slugify`）する。
//! - `snapshotVersion` は作品の版識別子（登録簿の `etag`）をそのまま使わず、
//!   取得時点の日付（UTC）で表す。これは契約の example（`'2026-11-20'`）の
//!   形式に合わせるためで、版の同一性判定には使わない。
//! - `novnow.minVersion` は hub が受理する novnow 形式の下限
//!   （`HUB_MINIMUM_CLIENT_VERSION`）を返す。
//! - `acquisition.url` は `{HUB_PUBLIC_BASE}/archives/{projectId}.zip` を
//!   機械生成する（契約 example と同じ形。実際のアーカイブ配信は構成側の
//!   責務であり、本ルーターは URL の発行のみを担う）。

use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::registry::{slugify, ProjectRecord};

use super::shared::{iso8601_date, resolve_project};

/// セットアップ URL のクエリ（契約 `u`。必須・uri・最大 2048 文字）。
#[derive(Debug, Deserialize)]
pub struct ResolveParams {
    pub u: String,
}

/// 取得元（契約 `SetupInfo.acquisition`）。
#[derive(Debug, Serialize)]
pub struct AcquisitionResponse {
    /// `archive` または `git`。
    pub kind: String,
    pub url: String,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// novnow アプリの要求（契約 `SetupInfo.novnow`）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NovnowRequirementResponse {
    pub min_version: String,
}

/// セットアップ情報（契約 `SetupInfo`）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInfoResponse {
    pub project_id: String,
    pub slug: String,
    pub snapshot_version: String,
    pub acquisition: AcquisitionResponse,
    pub novnow: NovnowRequirementResponse,
}

/// 公開アーカイブ URL（`acquisition.url`）を機械生成する。
fn archive_url(config: &crate::config::Config, project_id: &str) -> String {
    let base = config.hub_public_base.trim_end_matches('/');
    format!("{base}/archives/{project_id}.zip")
}

impl SetupInfoResponse {
    /// 登録簿レコードから `SetupInfo` を作る。
    fn from_record(record: &ProjectRecord, config: &crate::config::Config) -> Self {
        SetupInfoResponse {
            project_id: record.project_id.clone(),
            slug: slugify(&record.title),
            snapshot_version: iso8601_date(record.updated_at),
            acquisition: AcquisitionResponse {
                kind: "archive".to_string(),
                url: archive_url(config, &record.project_id),
                reference: Some(record.default_branch.clone()),
            },
            novnow: NovnowRequirementResponse {
                min_version: config.minimum_client_version.clone(),
            },
        }
    }
}

/// `GET /projects/{projectId}/setup`（契約 `getSetupInfo`、匿名可）。
pub async fn get_setup(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<SetupInfoResponse>, ApiError> {
    let record = resolve_project(&state, &project_id)?;
    Ok(Json(SetupInfoResponse::from_record(&record, &state.config)))
}

/// `GET /setup/resolve?u=…`（契約 `resolveSetupUrl`、匿名可）。
///
/// `u` はセットアップ URL。hub が発行する形式（`…/projects/{projectId}/setup`
/// または公開ページ `…/reading/?projectId={projectId}`）からプロジェクト
/// 識別子を取り出して解決する。識別子を取り出せない URL は `400`
/// （`validation_failed`）、識別子は取れるが未登録なら `404` とする。
pub async fn resolve(
    State(state): State<AppState>,
    params: Result<Query<ResolveParams>, QueryRejection>,
) -> Result<Json<SetupInfoResponse>, ApiError> {
    // `u` が欠落している場合は 400（契約の必須クエリ）。
    let Query(params) = params
        .map_err(|e| ApiError::ValidationFailed(format!("セットアップ URL が不正です: {e}")))?;
    if params.u.trim().is_empty() {
        return Err(ApiError::ValidationFailed(
            "セットアップ URL が空です".to_string(),
        ));
    }
    if params.u.chars().count() > 2048 {
        return Err(ApiError::ValidationFailed(
            "セットアップ URL が長すぎます（2048 文字以内）".to_string(),
        ));
    }
    let project_id = project_id_from_setup_url(&params.u).ok_or_else(|| {
        ApiError::ValidationFailed(format!(
            "セットアップ URL からプロジェクトを特定できません: {}",
            params.u
        ))
    })?;
    let record = resolve_project(&state, &project_id)?;
    Ok(Json(SetupInfoResponse::from_record(&record, &state.config)))
}

/// セットアップ URL からプロジェクト識別子を取り出す。
///
/// 受け付ける形式（いずれも hub が機械生成するもの）:
/// - `<base>/projects/{projectId}/setup`（`ProjectPage` が表示する URL）
/// - `<base>/reading/?projectId={projectId}`（公開ページ URL）
/// - `<base>/projects/?projectId={projectId}`（プロジェクトページ URL）
/// - `novnow://setup?projectId={projectId}`（デスクトップ版のカスタムスキーム）
///
/// クエリ文字列の `u` は URL エンコードされているため、呼び出し前に
/// axum がクエリを復号済みである（`Query` の展開時）。
fn project_id_from_setup_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // クエリ形式（`?projectId=…`／`?u=…`）を先に読む。
    if let Some(query) = trimmed.split_once('?').map(|(_, q)| q) {
        // さらに `u=` の中にネストした URL がある場合（セットアップ URL を
        // 二重に包んだ形）は内側を再帰的に解釈する。
        for pair in query.split('&') {
            let (key, value) = match pair.split_once('=') {
                Some(kv) => kv,
                None => continue,
            };
            let value = percent_decode(value);
            match key {
                "projectId" => {
                    if !value.is_empty() {
                        return Some(value);
                    }
                }
                "u" => {
                    if value != trimmed {
                        if let Some(inner) = project_id_from_setup_url(&value) {
                            return Some(inner);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // パス形式（`/projects/{projectId}/setup`）を読む。
    let path_only = trimmed.split(['?', '#']).next().unwrap_or(trimmed);
    let segments: Vec<&str> = path_only.split('/').filter(|s| !s.is_empty()).collect();
    for (index, segment) in segments.iter().enumerate() {
        if *segment != "projects" {
            continue;
        }
        // `/projects/{projectId}` の直後までを識別子として扱う。
        if let Some(candidate) = segments.get(index + 1) {
            if candidate.is_empty() || *candidate == "setup" {
                continue;
            }
            return Some((*candidate).to_string());
        }
    }
    None
}

/// 最小限のパーセントデコード（`%XX` のみ。`+` は空白として扱わない）。
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_project_id_from_setup_path() {
        assert_eq!(
            project_id_from_setup_url("https://hub.example.test/projects/prj_1/setup").as_deref(),
            Some("prj_1")
        );
    }

    #[test]
    fn extracts_project_id_from_reading_query() {
        assert_eq!(
            project_id_from_setup_url("https://hub.example.test/reading/?projectId=prj_2")
                .as_deref(),
            Some("prj_2")
        );
    }

    #[test]
    fn extracts_project_id_from_project_page_query() {
        assert_eq!(
            project_id_from_setup_url("https://hub.example.test/projects/?projectId=prj_3")
                .as_deref(),
            Some("prj_3")
        );
    }

    #[test]
    fn extracts_project_id_from_desktop_scheme() {
        assert_eq!(
            project_id_from_setup_url("novnow://setup?projectId=prj_4").as_deref(),
            Some("prj_4")
        );
    }

    #[test]
    fn extracts_project_id_from_nested_setup_url() {
        let nested = "https://hub.example.test/setup/resolve/?u=https%3A%2F%2Fhub.example.test%2Fprojects%2Fprj_5%2Fsetup";
        assert_eq!(project_id_from_setup_url(nested).as_deref(), Some("prj_5"));
    }

    #[test]
    fn rejects_unrelated_url() {
        assert_eq!(project_id_from_setup_url("https://example.com/about"), None);
    }

    #[test]
    fn rejects_empty_input() {
        assert_eq!(project_id_from_setup_url("   "), None);
    }

    #[test]
    fn percent_decode_handles_utf8_bytes() {
        assert_eq!(percent_decode("%E6%9C%88"), "月");
    }
}
