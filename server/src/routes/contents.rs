//! 作品ファイルの読み取り（契約 `getTree`／`getFile`）。
//!
//! - `GET /projects/{projectId}/tree`：ファイルツリー（`Tree`）を返す。
//! - `GET /projects/{projectId}/file?path=…`：ファイル内容（`FileContent`）を
//!   返す。
//!
//! どちらも契約で `security: []` のため匿名可。保管基盤（GitHub）の語彙は
//! `ContentPort` の背後に隠し、応答にはパス・種別・サイズ・版識別子のみを
//! 含める。
//!
//! 判断（`REF:docs/handoff.md` §4 判断 28）:
//! - `Tree.entries` は保管基盤の 1 階層分（ルート直下）を返す。全階層を
//!   平坦化すると契約の example（`chapters` の dir と `chapters/01.md` の
//!   file が併存）と食い違うため、階層はそのまま保つ。
//! - `Tree.truncated` は契約に `limit` が無いため常に `false` を返す。
//!   保管基盤側で 1 階層が切り詰められた場合は `true` を返す。
//! - ディレクトリの `size` は契約上必須のため、不明時は `0` を返す。
//! - `FileContent.content` は `encoding=utf8` のときだけ、`data` と `mime` は
//!   `encoding=base64` のときだけ返す（契約の説明に従う）。

use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::ports::{ContentPort, FileEncoding, TreeEntryKind};

use super::shared::{map_port_error, resolve_shell};

/// ファイル取得のクエリ（契約 `path`。必須・最大 1024 文字）。
#[derive(Debug, Deserialize)]
pub struct FileParams {
    pub path: String,
}

/// ツリーの要素（契約 `Tree.entries[]`）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntryResponse {
    pub path: String,
    /// `file` または `dir`。
    pub kind: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
}

/// ツリー（契約 `Tree`）。
#[derive(Debug, Serialize)]
pub struct TreeResponse {
    pub entries: Vec<TreeEntryResponse>,
    pub truncated: bool,
}

/// ファイル内容（契約 `FileContent`）。
#[derive(Debug, Serialize)]
pub struct FileContentResponse {
    pub path: String,
    /// `encoding=utf8` のときだけ返す。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// `encoding=base64` のときだけ返す。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// `encoding=base64` のときだけ返す。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    /// `utf8` または `base64`。
    pub encoding: String,
    pub etag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// `GET /projects/{projectId}/tree`（契約 `getTree`、匿名可）。
pub async fn get_tree(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<TreeResponse>, ApiError> {
    let shell_id = resolve_shell(&state, &project_id)?;
    let entries = state
        .github_adapter
        .list_tree(&shell_id, None)
        .await
        .map_err(map_port_error)?;

    let entries: Vec<TreeEntryResponse> = entries
        .into_iter()
        .map(|entry| TreeEntryResponse {
            etag: if entry.etag.is_empty() {
                None
            } else {
                Some(entry.etag.clone())
            },
            path: entry.path,
            kind: match entry.kind {
                TreeEntryKind::Directory => "dir".to_string(),
                TreeEntryKind::File => "file".to_string(),
            },
            size: entry.size.unwrap_or(0),
        })
        .collect();

    Ok(Json(TreeResponse {
        entries,
        truncated: false,
    }))
}

/// `GET /projects/{projectId}/file?path=…`（契約 `getFile`、匿名可）。
///
/// パスが保管基盤に無い場合は `404`。`path` の欠落・長すぎる場合は `400`。
pub async fn get_file(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    params: Result<Query<FileParams>, QueryRejection>,
) -> Result<Json<FileContentResponse>, ApiError> {
    let Query(params) =
        params.map_err(|e| ApiError::ValidationFailed(format!("path が不正です: {e}")))?;
    let path = params.path.trim().to_string();
    if path.is_empty() {
        return Err(ApiError::ValidationFailed(
            "path が指定されていません".to_string(),
        ));
    }
    if path.chars().count() > 1024 {
        return Err(ApiError::ValidationFailed(
            "path が長すぎます（1024 文字以内）".to_string(),
        ));
    }

    let shell_id = resolve_shell(&state, &project_id)?;
    let file = state
        .github_adapter
        .read_file(&shell_id, &path)
        .await
        .map_err(map_port_error)?;

    let encoding = match file.encoding {
        FileEncoding::Text => "utf8",
        FileEncoding::Base64 => "base64",
    };
    let (content, data, mime, size) = match file.encoding {
        FileEncoding::Text => {
            let size = file.content.len() as u64;
            (Some(file.content), None, None, Some(size))
        }
        FileEncoding::Base64 => {
            let size = decoded_size(&file.content);
            (
                None,
                Some(file.content),
                Some(mime_for_path(&path).to_string()),
                size,
            )
        }
    };

    Ok(Json(FileContentResponse {
        path,
        content,
        data,
        mime,
        encoding: encoding.to_string(),
        etag: file.etag,
        size,
    }))
}

/// base64 文字列が表す元データのバイト数を求める（改行・空白は無視）。
///
/// 契約 `FileContent.size` は任意項目のため、復号できない場合は `None` を返す。
fn decoded_size(data: &str) -> Option<u64> {
    let compact: Vec<u8> = data.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if compact.is_empty() || !compact.len().is_multiple_of(4) {
        return None;
    }
    let padding = compact.iter().rev().take_while(|b| **b == b'=').count();
    if padding > 2 {
        return None;
    }
    Some((compact.len() / 4 * 3 - padding) as u64)
}

/// パスから MIME 型を推定する（`encoding=base64` の応答用）。
///
/// デコード済みのバイト列を持たないため、拡張子からの推定に留める。
/// 未知の拡張子は汎用のバイナリ型を返す。
fn mime_for_path(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_size_accounts_for_padding_and_newlines() {
        // "hello" (5 バイト) の base64 は "aGVsbG8="。
        assert_eq!(decoded_size("aGVsbG8="), Some(5));
        // 空白・改行入りでも同じ。
        assert_eq!(decoded_size("aGVs\n bG8="), Some(5));
        assert_eq!(decoded_size(""), None);
        assert_eq!(decoded_size("abc"), None);
    }

    #[test]
    fn mime_is_guessed_from_extension() {
        assert_eq!(mime_for_path("images/cover.PNG"), "image/png");
        assert_eq!(
            mime_for_path("materials/index.json"),
            "application/octet-stream"
        );
        assert_eq!(mime_for_path("no-extension"), "application/octet-stream");
    }
}
