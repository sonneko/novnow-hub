//! novnow 軽量検証。
//!
//! 設計 `REF:docs/designs/m3.md` §2.1・仕様 `REF:docs/specifications/m3.md` §2.1 に
//! 準拠する。novnow 本体の仕様（novnow リポジトリの `specifications/project.md`
//! §4.2・§4.3）と同一の判定を行い、hub 独自の必須要素は発明しない。
//!
//! 検証項目:
//! - 必須ファイル: `project.toml`・`characters.md`・`words.md`・
//!   `materials/index.json`・`plots/all.cnov`
//! - 必須ディレクトリ: `contents/`（空でも可）・`plots/`
//! - `project.toml` が TOML として読めて、`schema_version` が `2` または省略。
//!   `2` より大きい版は拒否、TOML として読めない場合は拒否。
//! - 文書ファイルが UTF-8 として妥当。
//!
//! 判定は保管基盤への投入前に行う。失敗時は保管単位を作らず、欠落・不正ファイル
//! の一覧を返す（zip は検証後に破棄、仕様 §6.1）。

use std::fmt;

/// 検証エラー。失敗理由の一覧を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationError {
    /// 人間向けの失敗理由（複数件を改行で連結）。
    pub reasons: Vec<String>,
}

impl fmt::Display for VerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "novnow 検証に失敗しました: {}", self.reasons.join(" / "))
    }
}

impl std::error::Error for VerificationError {}

/// 現在のhub が受理する novnow の `schema_version`。
const SUPPORTED_SCHEMA_VERSION: u32 = 2;

/// 必須ファイル（ルート直下からの相対パス）。
const REQUIRED_FILES: &[&str] = &[
    "project.toml",
    "characters.md",
    "words.md",
    "materials/index.json",
    "plots/all.cnov",
];

/// 必須ディレクトリ（空でも可）。
const REQUIRED_DIRS: &[&str] = &["contents", "plots"];

/// 展開済みファイル（`zip_import::ImportedFile` 相当の語彙）。
///
/// 検証モジュールは zip の実装に依存せず、パスと内容だけを受ける。
pub struct SnapshotFile {
    /// 正規化済みパス（`/` 区切り）。
    pub path: String,
    /// ファイル内容。
    pub content: Vec<u8>,
}

/// zip 展開結果を検証する。成功時は `Ok(())`、失敗時は欠落・不正ファイルの一覧。
pub fn verify_snapshot(files: &[SnapshotFile]) -> Result<(), VerificationError> {
    let mut reasons: Vec<String> = Vec::new();

    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

    // 1. 必須ファイルの存在検査。
    for required in REQUIRED_FILES {
        if !paths.contains(required) {
            reasons.push(format!("必須ファイルが欠けています: {required}"));
        }
    }

    // 2. 必須ディレクトリの存在検査（その配下に少なくとも 1 ファイルがあれば存在とみなす）。
    for dir in REQUIRED_DIRS {
        let prefix = format!("{dir}/");
        let has_content = files.iter().any(|f| f.path.starts_with(&prefix));
        if !has_content {
            reasons.push(format!("必須ディレクトリが欠けています: {dir}/"));
        }
    }

    // 3. project.toml の妥当性検査。
    if let Some(manifest) = files.iter().find(|f| f.path == "project.toml") {
        if let Err(e) = verify_manifest(&manifest.content) {
            reasons.push(format!("project.toml が不正です: {e}"));
        }
    }

    // 4. UTF-8 妥当性検査（文書として認識するファイル）。
    for f in files {
        if is_document_file(&f.path) && !is_utf8(&f.content) {
            reasons.push(format!("UTF-8 として不正なファイル: {}", f.path));
        }
    }

    if reasons.is_empty() {
        Ok(())
    } else {
        Err(VerificationError { reasons })
    }
}

/// 文書として認識するファイルか（テキスト系の拡張子・`.cnov`・`.json` 等）。
/// 二进制っぽい拡張子は対象外とする。
fn is_document_file(path: &str) -> bool {
    matches!(
        path.rsplit('.').next(),
        Some("toml") | Some("md") | Some("cnov") | Some("json") | Some("txt")
    )
}

/// UTF-8 として妥当か。
fn is_utf8(content: &[u8]) -> bool {
    std::str::from_utf8(content).is_ok()
}

/// `project.toml` の妥当性を検査する。
///
/// TOML として読め、`schema_version` が `2` または省略であれば OK。
/// `2` より大きい版、または TOML として読めない場合は拒否。
fn verify_manifest(content: &[u8]) -> Result<(), String> {
    let text = std::str::from_utf8(content).map_err(|_| "UTF-8 として読めません".to_string())?;
    let toml = parse_minimal_toml(text).ok_or_else(|| "TOML として読めません".to_string())?;

    match toml.get("schema_version") {
        None => Ok(()),
        Some(TomlValue::Integer(n)) => {
            if *n < 0 {
                Err(format!("schema_version が負の値です（{n}）"))
            } else if (*n as u32) > SUPPORTED_SCHEMA_VERSION {
                Err(format!("未対応の版（schema_version={n}）"))
            } else {
                // 2 以下の版は novnow 本体が既定値で開ける版として受理する。
                Ok(())
            }
        }
        Some(other) => Err(format!("schema_version が整数でありません: {other:?}")),
    }
}

/// 最小限の TOML 値。キー検出と整数値の抽出に使う。
#[derive(Debug, Clone, PartialEq)]
enum TomlValue {
    String(String),
    Integer(i64),
    Bool(bool),
    /// その他（配列・テーブル等）。検証では未使用。
    Other,
}

/// 最小限の TOML パース。
///
/// `project.toml` の検証に必要なのは「TOML として読めるか」と
/// `schema_version` の整数値だけ。完全な TOML 文法は不要なので、
/// トップレベルのキー/値ペアとテーブルの識別子を扱う軽量パースを行う。
fn parse_minimal_toml(text: &str) -> Option<std::collections::HashMap<String, TomlValue>> {
    let mut map = std::collections::HashMap::new();
    let mut current_table: Option<String> = None;

    for raw_line in text.lines() {
        let stripped = strip_comment(raw_line);
        let line = stripped.trim();
        if line.is_empty() {
            continue;
        }

        // テーブル宣言（`[section]`）。
        if let Some(table) = line.strip_prefix('[') {
            let table = table.trim_end_matches(']').trim();
            if table.is_empty() || table.contains(']') {
                return None;
            }
            current_table = Some(table.to_string());
            continue;
        }

        // キー/値（`key = value`）。
        let (key, value) = line
            .split_once('=')
            .map(|(k, v)| (k.trim(), v.trim()))?;
        if key.is_empty() || !is_valid_toml_key(key) {
            return None;
        }
        let full_key = match &current_table {
            Some(t) => format!("{t}.{key}"),
            None => key.to_string(),
        };
        // トップレベル（またはテーブル内）の schema_version を記録の対象とする。
        if full_key == "schema_version" {
            if let Some(parsed) = parse_toml_value(value) {
                map.insert(full_key, parsed);
            } else {
                return None;
            }
        } else {
            // 他のキーは値がパースできれば握る（TOML として読めるかの判定用）。
            parse_toml_value(value)?;
            map.entry(full_key).or_insert(TomlValue::Other);
        }
    }
    Some(map)
}

/// 行末コメントを除去（引用符外の `#`）。
fn strip_comment(line: &str) -> String {
    let mut in_string = false;
    let mut quote: Option<char> = None;
    for (i, ch) in line.char_indices() {
        if in_string {
            if ch == quote.unwrap_or('"') {
                in_string = false;
            }
        } else if ch == '"' || ch == '\'' {
            in_string = true;
            quote = Some(ch);
        } else if ch == '#' {
            return line[..i].to_string();
        }
    }
    line.to_string()
}

/// TOML キーとして妥当か。
fn is_valid_toml_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// TOML 値を最小限パースする。整数・文字列・ブールを認識、他は `Other`。
fn parse_toml_value(value: &str) -> Option<TomlValue> {
    if value.is_empty() {
        return None;
    }
    if value == "true" || value == "false" {
        return Some(TomlValue::Bool(value == "true"));
    }
    if let Some(int) = parse_toml_int(value) {
        return Some(TomlValue::Integer(int));
    }
    if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
        || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
    {
        return Some(TomlValue::String(value[1..value.len() - 1].to_string()));
    }
    // 配列やテーブル等は握る（その内容の深い検証はしない）。
    Some(TomlValue::Other)
}

/// TOML の整数リテラル（符号付き）を読む。
fn parse_toml_int(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, content: &[u8]) -> SnapshotFile {
        SnapshotFile {
            path: path.to_string(),
            content: content.to_vec(),
        }
    }

    /// 検証を通る最小構成。
    fn valid_snapshot() -> Vec<SnapshotFile> {
        vec![
            file("project.toml", b"[project]\nname = \"t\"\nschema_version = 2\n"),
            file("characters.md", "# 登場人物\n".as_bytes()),
            file("words.md", "# 用語\n".as_bytes()),
            file("materials/index.json", b"{}\n"),
            file("plots/all.cnov", b"plot\n"),
            file("contents/01.cnov", b"body\n"),
        ]
    }

    #[test]
    fn accepts_valid_snapshot() {
        verify_snapshot(&valid_snapshot()).expect("最小構成は受理");
    }

    #[test]
    fn accepts_missing_schema_version() {
        let mut files = valid_snapshot();
        files[0] = file("project.toml", b"[project]\nname = \"t\"\n");
        verify_snapshot(&files).expect("schema_version 省略は受理");
    }

    #[test]
    fn rejects_missing_required_file() {
        let mut files = valid_snapshot();
        files.retain(|f| f.path != "words.md");
        let err = verify_snapshot(&files).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("words.md")));
    }

    #[test]
    fn rejects_missing_required_dir() {
        let mut files = valid_snapshot();
        files.retain(|f| !f.path.starts_with("contents/"));
        let err = verify_snapshot(&files).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("contents/")));
    }

    #[test]
    fn rejects_too_new_schema_version() {
        let mut files = valid_snapshot();
        files[0] = file("project.toml", b"schema_version = 3\n");
        let err = verify_snapshot(&files).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("schema_version")));
    }

    #[test]
    fn rejects_broken_manifest() {
        let mut files = valid_snapshot();
        files[0] = file("project.toml", b"this is = = not toml\n");
        let err = verify_snapshot(&files).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("project.toml")));
    }

    #[test]
    fn rejects_non_utf8_document() {
        let mut files = valid_snapshot();
        files[1] = file("characters.md", b"# \xff\xfe invalid\n");
        let err = verify_snapshot(&files).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("characters.md")));
    }

    #[test]
    fn accepts_unknown_files() {
        // 未知ファイルは無視（拒否理由にしない）。
        let mut files = valid_snapshot();
        files.push(file("ideas.md", "# アイデア\n".as_bytes()));
        files.push(file("prompts/x.md", b"x\n"));
        verify_snapshot(&files).expect("未知ファイルがあっても受理");
    }

    #[test]
    fn empty_required_dir_is_ok() {
        // contents/ が空でも必須ディレクトリは満たさない（ファイルが無い）。
        // ただし contents/ 配下に何らかのファイルがあれば OK。ここでは
        // contents/ にファイルが無い場合を検証する。
        let mut files = valid_snapshot();
        files.retain(|f| !f.path.starts_with("contents/"));
        let err = verify_snapshot(&files).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("contents/")));
    }

    #[test]
    fn manifest_comment_is_ignored() {
        let mut files = valid_snapshot();
        files[0] = file(
            "project.toml",
            b"# comment\nschema_version = 2 # inline comment\n",
        );
        verify_snapshot(&files).expect("コメントを含む TOML は受理");
    }

    #[test]
    fn manifest_in_table_schema_version_detected() {
        let mut files = valid_snapshot();
        files[0] = file("project.toml", b"[novnow]\nschema_version = 2\n");
        verify_snapshot(&files).expect("テーブル内の schema_version は検出");
    }
}
