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
///
/// `dirs` は zip 内のディレクトリ entry の一覧（末尾 `/` なし）。必須ディレクトリ
/// の存在はファイルの有無だけでなくディレクトリ entry も含めて判定する
/// （`contents/` は空でも受理する）。
pub fn verify_snapshot(
    files: &[SnapshotFile],
    dirs: &[String],
) -> Result<(), VerificationError> {
    let mut reasons: Vec<String> = Vec::new();

    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

    // 1. 必須ファイルの存在検査。
    for required in REQUIRED_FILES {
        if !paths.contains(required) {
            reasons.push(format!("必須ファイルが欠けています: {required}"));
        }
    }

    // 2. 必須ディレクトリの存在検査。ファイルの有無（配下にファイル）か
    //    ディレクトリ entry の存在のいずれかがあれば存在とみなす（空でも可）。
    for dir in REQUIRED_DIRS {
        let prefix = format!("{dir}/");
        let has_content = files.iter().any(|f| f.path.starts_with(&prefix));
        let has_dir_entry = dirs.iter().any(|d| d == dir || d.starts_with(&prefix));
        if !has_content && !has_dir_entry {
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

/// 引用符状態。マルチライン文字列（三重引用符）の継続を行間で追跡する。
#[derive(Clone, Copy, PartialEq)]
enum QuoteState {
    /// 文字列外。
    None,
    /// 単一行文字列中（`"` または `'`）。
    Single(char),
    /// マルチライン文字列中（`"""` または `'''`）。
    Triple(char),
}

/// 1 行を走査する。行末コメント（引用符外の `#`）を除去しつつ、
/// `[`・`]` の深さ増分と行末の引用符状態を返す。
///
/// 引数 `incoming` は前行から引き継ぐ引用符状態。マルチライン文字列の
/// 継続行も正しく扱うため、行単独ではなくこの状態を渡して走査する。
/// 返り値の `depth` は行内の `[`・`]` の増分（行頭を 0 として集計）。
/// `state` は行末の引用符状態（マルチライン文字列中なら `Triple`/`Single`）。
fn scan_line(line: &str, incoming: QuoteState) -> (String, i64, QuoteState) {
    let mut cleaned = String::new();
    let mut depth: i64 = 0;
    let mut state = incoming;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match state {
            QuoteState::None => {
                // 三重引用符（マルチライン文字列）の開始を先に検出。
                if i + 2 < chars.len()
                    && chars[i] == '"'
                    && chars[i + 1] == '"'
                    && chars[i + 2] == '"'
                {
                    state = QuoteState::Triple('"');
                    cleaned.push_str("\"\"\"");
                    i += 3;
                } else if i + 2 < chars.len()
                    && chars[i] == '\''
                    && chars[i + 1] == '\''
                    && chars[i + 2] == '\''
                {
                    state = QuoteState::Triple('\'');
                    cleaned.push_str("'''");
                    i += 3;
                } else if chars[i] == '"' || chars[i] == '\'' {
                    state = QuoteState::Single(chars[i]);
                    cleaned.push(chars[i]);
                    i += 1;
                } else if chars[i] == '[' {
                    depth += 1;
                    cleaned.push(chars[i]);
                    i += 1;
                } else if chars[i] == ']' {
                    depth -= 1;
                    cleaned.push(chars[i]);
                    i += 1;
                } else if chars[i] == '#' {
                    // 引用符外の `#` は行末コメント。以降は無視。
                    break;
                } else {
                    cleaned.push(chars[i]);
                    i += 1;
                }
            }
            QuoteState::Single(q) => {
                if chars[i] == q {
                    state = QuoteState::None;
                }
                cleaned.push(chars[i]);
                i += 1;
            }
            QuoteState::Triple(q) => {
                // 三重引用符の終了を先に検出（`"""` / `'''`）。
                if i + 2 < chars.len() && chars[i] == q && chars[i + 1] == q && chars[i + 2] == q {
                    state = QuoteState::None;
                    cleaned.push_str(&chars[i..i + 3].iter().collect::<String>());
                    i += 3;
                } else {
                    cleaned.push(chars[i]);
                    i += 1;
                }
            }
        }
    }
    (cleaned, depth, state)
}

/// 最小限の TOML パース。
///
/// `project.toml` の検証に必要なのは「TOML として読めるか」と
/// トップレベルの `schema_version` の整数値だけ。完全な TOML 文法は不要。
/// 複数行の配列・文字列を含む TOML も読めるように、`[`・`]` の深さと
/// 引用符の状態を行間で追跡して、トップレベルのキー/値・テーブル宣言を
/// 識別する。マルチラインの値（配列・文字列）は深さが 0 に戻るまで読み
/// 飛ばし、平衡のみを検証する。
fn parse_minimal_toml(text: &str) -> Option<std::collections::HashMap<String, TomlValue>> {
    let mut map = std::collections::HashMap::new();
    // 文書レベルの深さ（`[`／`]` の集計）。
    let mut depth: i64 = 0;
    let mut state: QuoteState = QuoteState::None;
    let mut current_table: Option<String> = None;

    for raw_line in text.lines() {
        // 行末コメント除去＋深さ増分＋行末引用符状態を一括取得。
        let (cleaned, line_depth, end_state) = scan_line(raw_line, state);
        let start_state = state;
        state = end_state;
        let line = cleaned.trim();

        let start_depth = depth;

        // マルチライン文字列の継続行。値の検証はスキップ。
        if start_state != QuoteState::None {
            depth = start_depth + line_depth;
            continue;
        }

        // 空行はスキップ（深さ変化は 0）。
        if line.is_empty() {
            depth = start_depth + line_depth;
            continue;
        }

        // トップレベル（文書深さ 0）の行のみを文法的に検証・記録する。
        if start_depth == 0 {
            if line.starts_with('[') {
                // テーブル宣言（`[section]`）か、配列値の先頭か。
                // 行末で深さが 0 に戻っていない（配列の始まり）なら継続行へ。
                if line_depth > 0 {
                    depth = start_depth + line_depth;
                    continue;
                }
                // テーブル宣言。
                let table = line
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .trim();
                if table.is_empty() || table.contains('[') || table.contains(']') {
                    return None;
                }
                current_table = Some(table.to_string());
            } else if line.contains('=') {
                let (key, value) = line
                    .split_once('=')
                    .map(|(k, v)| (k.trim(), v.trim()))?;
                if key.is_empty() || !is_valid_toml_key(key) {
                    return None;
                }
                // schema_version の検査はトップレベルのキーのみ（novnow 仕様 §4.5）。
                // テーブル内の schema_version は検査対象外（省略扱い）とする。
                let is_top_level = current_table.is_none();
                if is_top_level && key == "schema_version" {
                    if let Some(parsed) = parse_toml_value(value) {
                        map.insert(key.to_string(), parsed);
                    } else {
                        return None;
                    }
                } else if line_depth > 0 {
                    // マルチライン値（配列等）の先頭。平衡のみ検証。
                    parse_toml_value(value)?;
                } else {
                    // 単一行の値はパースできれば握る（TOML として読めるかの判定用）。
                    parse_toml_value(value)?;
                    let full_key = match &current_table {
                        Some(t) => format!("{t}.{key}"),
                        None => key.to_string(),
                    };
                    map.entry(full_key).or_insert(TomlValue::Other);
                }
            }
        }

        depth = start_depth + line_depth;
    }

    // 深さが 0 に戻っていない・文字列が閉じていない場合は不正。
    if depth != 0 || state != QuoteState::None {
        return None;
    }
    Some(map)
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
            // novnow の実際の形式: schema_version はトップレベル。
            file("project.toml", b"schema_version = 2\n[project]\nname = \"t\"\n"),
            file("characters.md", "# 登場人物\n".as_bytes()),
            file("words.md", "# 用語\n".as_bytes()),
            file("materials/index.json", b"{}\n"),
            file("plots/all.cnov", b"plot\n"),
            file("contents/01.cnov", b"body\n"),
        ]
    }

    /// 検証を通るディレクトリ一覧（空の状態）。
    fn empty_dirs() -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn accepts_valid_snapshot() {
        verify_snapshot(&valid_snapshot(), &empty_dirs()).expect("最小構成は受理");
    }

    #[test]
    fn accepts_missing_schema_version() {
        let mut files = valid_snapshot();
        files[0] = file("project.toml", b"[project]\nname = \"t\"\n");
        verify_snapshot(&files, &empty_dirs()).expect("schema_version 省略は受理");
    }

    #[test]
    fn rejects_missing_required_file() {
        let mut files = valid_snapshot();
        files.retain(|f| f.path != "words.md");
        let err = verify_snapshot(&files, &empty_dirs()).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("words.md")));
    }

    #[test]
    fn rejects_missing_required_dir() {
        // contents/ のファイルもディレクトリ entry も無い場合は拒否。
        let mut files = valid_snapshot();
        files.retain(|f| !f.path.starts_with("contents/"));
        let err = verify_snapshot(&files, &empty_dirs()).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("contents/")));
    }

    #[test]
    fn rejects_too_new_schema_version() {
        let mut files = valid_snapshot();
        files[0] = file("project.toml", b"schema_version = 3\n");
        let err = verify_snapshot(&files, &empty_dirs()).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("schema_version")));
    }

    #[test]
    fn rejects_broken_manifest() {
        let mut files = valid_snapshot();
        files[0] = file("project.toml", b"this is = = not toml\n");
        let err = verify_snapshot(&files, &empty_dirs()).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("project.toml")));
    }

    #[test]
    fn rejects_non_utf8_document() {
        let mut files = valid_snapshot();
        files[1] = file("characters.md", b"# \xff\xfe invalid\n");
        let err = verify_snapshot(&files, &empty_dirs()).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("characters.md")));
    }

    #[test]
    fn accepts_unknown_files() {
        // 未知ファイルは無視（拒否理由にしない）。
        let mut files = valid_snapshot();
        files.push(file("ideas.md", "# アイデア\n".as_bytes()));
        files.push(file("prompts/x.md", b"x\n"));
        verify_snapshot(&files, &empty_dirs()).expect("未知ファイルがあっても受理");
    }

    #[test]
    fn empty_required_dir_with_dir_entry_is_ok() {
        // `contents/` は空でも受理する。ファイルが無くディレクトリ entry のみ
        // の場合も、dirs に `contents` があれば必須ディレクトリは満たされる。
        let mut files = valid_snapshot();
        files.retain(|f| !f.path.starts_with("contents/"));
        let dirs = vec!["contents".to_string()];
        verify_snapshot(&files, &dirs).expect("空の contents/ は受理");
    }

    #[test]
    fn manifest_comment_is_ignored() {
        let mut files = valid_snapshot();
        files[0] = file(
            "project.toml",
            b"# comment\nschema_version = 2 # inline comment\n",
        );
        verify_snapshot(&files, &empty_dirs()).expect("コメントを含む TOML は受理");
    }

    #[test]
    fn table_schema_version_is_ignored_top_level_only() {
        // schema_version の検査はトップレベルのみ。テーブル内（[novnow] など）の
        // schema_version は無視され、省略扱いとなる（novnow 本体も同様）。
        // novnow の仕様 §4.5 では schema_version はトップレベルに置くもの。
        //
        // トップレベルが省略でも、テーブル内に schema_version = 3 があっても
        // 受理される（テーブル内は検査対象外＝省略扱い）。これは仕様どおり。
        let mut files = valid_snapshot();
        files[0] = file("project.toml", b"[novnow]\nschema_version = 3\n");
        verify_snapshot(&files, &empty_dirs()).expect("テーブル内の版は検査対象外（省略扱い）");
    }

    #[test]
    fn accepts_multiline_array_manifest() {
        // 複数行の配列を含む project.toml も TOML として読めること。
        // schema_version = 2 がトップレベルにあれば受理する。
        let manifest = b"schema_version = 2\n\ncustom_roles = [\n    \"a\",\n    \"b\",\n    \"c\",\n]\n";
        let mut files = valid_snapshot();
        files[0] = file("project.toml", manifest);
        verify_snapshot(&files, &empty_dirs()).expect("複数行配列を含む TOML は受理");
    }

    #[test]
    fn rejects_multiline_array_with_too_new_schema_version() {
        // 複数行の配列と schema_version = 3 を併記した場合は版が新しく拒否される。
        let manifest = b"schema_version = 3\n\ncustom_roles = [\n    \"a\",\n    \"b\",\n]\n";
        let mut files = valid_snapshot();
        files[0] = file("project.toml", manifest);
        let err = verify_snapshot(&files, &empty_dirs()).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("schema_version")));
    }

    #[test]
    fn accepts_multiline_string_manifest() {
        // 複数行文字列（三重引用符の基本文字列）を含む project.toml も TOML として読めること。
        let manifest = b"schema_version = 2\n\ndescription = \"\"\"\nline1\nline2\n\"\"\"\n";
        let mut files = valid_snapshot();
        files[0] = file("project.toml", manifest);
        verify_snapshot(&files, &empty_dirs()).expect("複数行文字列を含む TOML は受理");
    }

    #[test]
    fn rejects_unbalanced_multiline_array() {
        // 複数行配列が閉じていない（不平衡）場合は TOML として読めず拒否。
        let manifest = b"schema_version = 2\n\ncustom_roles = [\n    \"a\",\n";
        let mut files = valid_snapshot();
        files[0] = file("project.toml", manifest);
        let err = verify_snapshot(&files, &empty_dirs()).unwrap_err();
        assert!(err.reasons.iter().any(|r| r.contains("project.toml")));
    }
}
