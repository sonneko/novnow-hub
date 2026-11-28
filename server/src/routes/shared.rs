//! ルーター間で共有する小さなヘルパー。
//!
//! `routes/projects.rs`・`routes/collab.rs`・`routes/verify.rs` はそれぞれ
//! 同等のヘルパーを内部に持つが、本モジュールは後続で追加した
//! 読み取り系（contents／reading／setup／releases）の 4 ルーターで共有する
//! ために置く（既存ルーターの重複解消は本タスクの範囲外）。

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::ports::PortError;
use crate::registry::ProjectRecord;

/// projectId を保管単位名（shell_id）へ解決する。未登録なら 404。
///
/// `REF:docs/designs/api.md` の公開読み取りと同じ扱いとし、GitHub の
/// 語彙（リポジトリ名等）は応答に一切含めない。
pub fn resolve_shell(state: &AppState, project_id: &str) -> Result<String, ApiError> {
    state
        .projects
        .get(project_id)
        .map(|r| r.shell_id)
        .ok_or_else(|| ApiError::NotFound(format!("プロジェクトが見つかりません: {project_id}")))
}

/// projectId から登録簿のレコードへ解決する。未登録なら 404。
pub fn resolve_project(state: &AppState, project_id: &str) -> Result<ProjectRecord, ApiError> {
    state
        .projects
        .get(project_id)
        .ok_or_else(|| ApiError::NotFound(format!("プロジェクトが見つかりません: {project_id}")))
}

/// ポートエラーを API エラーへ写像する。
pub fn map_port_error(e: PortError) -> ApiError {
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

/// UNIX 秒を ISO8601（UTC）へ変換。
pub fn iso8601(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// UNIX 秒を日付（UTC・`YYYY-MM-DD`）へ変換。
pub fn iso8601_date(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{register_test_project, test_state};

    #[test]
    fn resolve_shell_returns_shell_id_for_registered_project() {
        let state = test_state();
        register_test_project(&state, "prj_1", "octo/demo");
        assert_eq!(resolve_shell(&state, "prj_1").unwrap(), "octo/demo");
    }

    #[test]
    fn resolve_shell_is_not_found_for_unknown_project() {
        let state = test_state();
        assert!(matches!(
            resolve_shell(&state, "nope"),
            Err(ApiError::NotFound(_))
        ));
    }

    #[test]
    fn iso8601_date_formats_epoch_day() {
        assert_eq!(iso8601_date(1767225600), "2026-01-01");
    }

    #[test]
    fn iso8601_formats_utc_timestamp() {
        assert_eq!(iso8601(1767225600), "2026-01-01T00:00:00Z");
    }
}
