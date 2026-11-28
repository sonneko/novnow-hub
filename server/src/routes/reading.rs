//! 読者ビュー本文（契約 `getReadingView`）。
//!
//! `GET /projects/{projectId}/reading` は、読者向けに整形した本文を
//! `ReadingDocument`（タイトル＋構造化ブロック配列＋版識別子）で返す。
//! 契約で `security: []` のため匿名可。
//!
//! 本文は `contents/` 配下の本文ファイル（`.cnov`・`.md`・`.txt`）を
//! パス順に連結して構成する。保管基盤（GitHub）の語彙は `ContentPort` の
//! 背後に隠す。
//!
//! 判断（`REF:docs/handoff.md` §4 判断 28）:
//! - novnow 本文の構文仕様は本リポジトリに含まれないため、hub は独自の
//!   構文解析を発明しない。行単位の最小写像（`#` 見出し → `heading`、
//!   区切り記号のみの行 → `scene_break`、その他の非空行 → `paragraph`）に
//!   留め、`speaker` を要する `dialogue` は生成しない。
//! - したがって注釈（コメント）の除去は行わない。除去には novnow 本文の
//!   注釈記法の仕様が必要であり、本リポジトリ外の情報のため実装しない。
//!   この点は既知の残課題として記録する（契約は変更しない）。
//! - `etag` は本文の内容から算出する（`sha256` の hex）。内容が変われば
//!   必ず変わり、フロントの ETag キャッシュ（`REF:web/src/lib/reader-cache.ts`）
//!   の鍵として使える。
//! - `title` は登録簿のタイトル（作品名）を返す。

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::app_state::AppState;
use crate::error::ApiError;
use crate::ports::{ContentPort, TreeEntryKind};

use super::shared::{map_port_error, resolve_project, resolve_shell};

/// 本文として扱う拡張子。
const BODY_EXTENSIONS: &[&str] = &["cnov", "md", "txt"];

/// 読者ビューのブロック（契約 `ReadingDocument.blocks[]`）。
#[derive(Debug, Serialize)]
pub struct BlockResponse {
    /// `heading`・`paragraph`・`dialogue`・`scene_break`。
    pub kind: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

/// 読者ビュー（契約 `ReadingDocument`）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingDocumentResponse {
    pub project_id: String,
    pub title: String,
    pub blocks: Vec<BlockResponse>,
    pub etag: String,
}

/// `GET /projects/{projectId}/reading`（契約 `getReadingView`、匿名可）。
pub async fn get_reading(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<ReadingDocumentResponse>, ApiError> {
    let record = resolve_project(&state, &project_id)?;
    let shell_id = resolve_shell(&state, &project_id)?;

    let mut body_paths = collect_body_paths(&state, &shell_id).await?;
    body_paths.sort();

    let mut body = String::new();
    for path in &body_paths {
        let file = state
            .github_adapter
            .read_file(&shell_id, path)
            .await
            .map_err(map_port_error)?;
        body.push_str(&file.content);
        if !body.ends_with('\n') {
            body.push('\n');
        }
    }

    let etag = content_etag(&body);
    let blocks = to_blocks(&body);

    Ok(Json(ReadingDocumentResponse {
        project_id: record.project_id,
        title: record.title,
        blocks,
        etag,
    }))
}

/// `contents/` 配下の本文ファイルのパスを集める（全階層）。
async fn collect_body_paths(state: &AppState, shell_id: &str) -> Result<Vec<String>, ApiError> {
    let mut paths: Vec<String> = Vec::new();
    let mut stack: Vec<String> = vec!["contents".to_string()];
    while let Some(prefix) = stack.pop() {
        let entries = state
            .github_adapter
            .list_tree(shell_id, Some(prefix.as_str()))
            .await
            .map_err(map_port_error)?;
        for entry in entries {
            match entry.kind {
                TreeEntryKind::Directory => stack.push(entry.path),
                TreeEntryKind::File => {
                    if is_body_path(&entry.path) {
                        paths.push(entry.path);
                    }
                }
            }
        }
    }
    Ok(paths)
}

/// 本文ファイルか（拡張子で判定）。
fn is_body_path(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    BODY_EXTENSIONS.contains(&ext.as_str())
}

/// 本文の内容から版識別子を作る。
fn content_etag(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 本文を行単位の最小写像でブロック列へ変換する。
fn to_blocks(body: &str) -> Vec<BlockResponse> {
    let mut blocks: Vec<BlockResponse> = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();

    fn flush(blocks: &mut Vec<BlockResponse>, paragraph: &mut Vec<String>) {
        if paragraph.is_empty() {
            return;
        }
        blocks.push(BlockResponse {
            kind: "paragraph".to_string(),
            text: paragraph.join("\n").trim().to_string(),
            speaker: None,
        });
        paragraph.clear();
    }

    for raw in body.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut blocks, &mut paragraph);
            continue;
        }
        if is_scene_break(trimmed) {
            flush(&mut blocks, &mut paragraph);
            blocks.push(BlockResponse {
                kind: "scene_break".to_string(),
                text: String::new(),
                speaker: None,
            });
            continue;
        }
        if let Some(rest) = heading_text(trimmed) {
            flush(&mut blocks, &mut paragraph);
            blocks.push(BlockResponse {
                kind: "heading".to_string(),
                text: rest,
                speaker: None,
            });
            continue;
        }
        paragraph.push(line.to_string());
    }
    flush(&mut blocks, &mut paragraph);
    blocks
}

/// Markdown／novnow 風の見出し行か（`#` 1〜6 個＋空白＋本文）。
fn heading_text(line: &str) -> Option<String> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    if !rest.starts_with(' ') && !rest.is_empty() {
        return None;
    }
    let text = rest.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.to_string())
}

/// 場面転換の区切り行か（`---`・`***`・`＊ ＊ ＊` 等、同種の記号のみの行）。
fn is_scene_break(line: &str) -> bool {
    let chars: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() < 3 {
        return false;
    }
    let first = chars[0];
    if !matches!(first, '-' | '*' | '＊' | '_' | '─' | '―') {
        return false;
    }
    chars.iter().all(|c| *c == first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_headings_paragraphs_and_scene_breaks() {
        let body = "# 第一章\n\n夜の庭は静かでした。\n二行目です。\n\n＊ ＊ ＊\n\nおわり。\n";
        let blocks = to_blocks(body);
        let kinds: Vec<&str> = blocks.iter().map(|b| b.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec!["heading", "paragraph", "scene_break", "paragraph"]
        );
        assert_eq!(blocks[0].text, "第一章");
        assert_eq!(blocks[1].text, "夜の庭は静かでした。\n二行目です。");
        assert_eq!(blocks[2].text, "");
        assert_eq!(blocks[3].text, "おわり。");
    }

    #[test]
    fn does_not_emit_dialogue_without_speaker() {
        let blocks = to_blocks("「誰かいるの？」\n");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, "paragraph");
        assert!(blocks[0].speaker.is_none());
    }

    #[test]
    fn hash_without_space_is_not_a_heading() {
        assert_eq!(heading_text("#タグ"), None);
        assert_eq!(heading_text("# 見出し"), Some("見出し".to_string()));
        assert_eq!(heading_text("####### 見出し"), None);
    }

    #[test]
    fn scene_break_requires_same_symbol() {
        assert!(is_scene_break("- - -"));
        assert!(is_scene_break("***"));
        assert!(!is_scene_break("--*"));
        assert!(!is_scene_break("--"));
    }

    #[test]
    fn etag_changes_with_content() {
        assert_ne!(content_etag("a"), content_etag("b"));
        assert_eq!(content_etag("a"), content_etag("a"));
        assert_eq!(content_etag("a").len(), 64);
    }

    #[test]
    fn body_paths_are_limited_to_body_extensions() {
        assert!(is_body_path("contents/01.cnov"));
        assert!(is_body_path("contents/sub/02.MD"));
        assert!(!is_body_path("contents/cover.png"));
        assert!(!is_body_path("project.toml"));
    }
}
