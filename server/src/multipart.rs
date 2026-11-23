//! multipart/form-data のパース（純粋 Rust 自己実装）。
//!
//! 判断（`REF:docs/handoff.md` §4 判断 4）により、サンドボックスはオフライン
//! ビルドで `multer` 等の crate が使えないため、`Content-Type` の boundary を
//! 基にパースを自前実装する。外部 crate に依存しない。
//!
//! 仕様（RFC 2046 / 7578）に準拠する:
//! - 各パートは `--<boundary>` で区切られ、末尾は `--<boundary>--`。
//! - パート間は CRLF（`\r\n`）で区切られる。
//! - 各パートは CRLF 区切りのヘッダ列と、空行（CRLF）の後のボディを持つ。
//! - `Content-Disposition` の `name` からフィールド名、`filename` の有無で
//!   ファイルパートを判定する。

use std::fmt;

/// multipart パースエラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartError {
    /// Content-Type が multipart/form-data でない、または boundary が無い。
    MissingBoundary,
    /// 境界が見つからない、または本体が不正。
    InvalidBody(String),
    /// フィールドが欠けている。
    MissingField(String),
}

impl fmt::Display for MultipartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MultipartError::MissingBoundary => write!(f, "multipart の boundary が指定されていません"),
            MultipartError::InvalidBody(d) => write!(f, "multipart 本体が不正です: {d}"),
            MultipartError::MissingField(n) => write!(f, "必須フィールドが欠けています: {n}"),
        }
    }
}

impl std::error::Error for MultipartError {}

/// Content-Type から boundary を抽出する。
///
/// `multipart/form-data; boundary=----abc` の形から `----abc` を返す。
/// boundary は引用符で囲まれる場合もある。
pub fn boundary_from_content_type(content_type: &str) -> Result<String, MultipartError> {
    let lower = content_type.to_ascii_lowercase();
    if !lower.starts_with("multipart/form-data") {
        return Err(MultipartError::MissingBoundary);
    }
    for param in content_type.split(';').skip(1) {
        let param = param.trim();
        if let Some(rest) = param.strip_prefix("boundary=") {
            let value = rest.trim();
            // 引用符で囲まれる場合を外す。
            let value = value
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            if value.is_empty() {
                return Err(MultipartError::MissingBoundary);
            }
            return Ok(value.to_string());
        }
    }
    Err(MultipartError::MissingBoundary)
}

/// パースした 1 つのパート。
#[derive(Debug, Clone)]
pub struct Part {
    /// フィールド名（`Content-Disposition` の `name`）。
    pub name: String,
    /// ファイル名（`Content-Disposition` の `filename`。ファイルパートのみ）。
    pub filename: Option<String>,
    /// パートのボディ。
    pub content: Vec<u8>,
}

impl Part {
    /// テキストとして解釈する（UTF-8 として妥当でない場合は置換）。
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.content).into_owned()
    }
}

/// Content-Type の boundary と本体から全パートをパースする。
///
/// `body` は HTTP ボディの完全な内容（先頭の preamble と末尾の epilogue を含む）。
pub fn parse(content_type: &str, body: &[u8]) -> Result<Vec<Part>, MultipartError> {
    let boundary = boundary_from_content_type(content_type)?;
    let delimiter = format!("--{boundary}");
    let delimiter_bytes = delimiter.as_bytes();

    let mut parts = Vec::new();

    // 最初の境界を探す。preamble は無視する。`pos` は境界の先頭を指す。
    let mut pos = find_subslice(body, delimiter_bytes).ok_or_else(|| {
        MultipartError::InvalidBody("先頭の境界が見つかりません".into())
    })?;

    loop {
        // 現在の境界がクローズ境界（`--<boundary>--`）か判定する。
        // 境界の先頭は `--` で始まるため、直後の 2 バイトが `--` なら終了。
        if body[pos + delimiter_bytes.len()..].starts_with(b"--") {
            break;
        }
        // 境界直後は CRLF。
        let after = pos + delimiter_bytes.len();
        if !body[after..].starts_with(b"\r\n") {
            return Err(MultipartError::InvalidBody(
                "境界の後に CRLF がありません".into(),
            ));
        }
        let mut cur = after + 2;

        // パートのヘッダとボディを分離。ヘッダは空行（CRLF CRLF）で終わる。
        let header_end = find_subslice(&body[cur..], b"\r\n\r\n").ok_or_else(|| {
            MultipartError::InvalidBody("パートのヘッダ区切りが見つかりません".into())
        })?;
        let header_bytes = &body[cur..cur + header_end];
        let header_text = std::str::from_utf8(header_bytes)
            .map_err(|_| MultipartError::InvalidBody("パートヘッダが UTF-8 でない".into()))?;
        cur = cur + header_end + 4;

        // 次の境界までがこのパートのボディ。
        let next = find_subslice(&body[cur..], delimiter_bytes).ok_or_else(|| {
            MultipartError::InvalidBody("パートの終了境界が見つかりません".into())
        })?;
        let content = body[cur..cur + next].to_vec();
        // ボディ末尾の CRLF（境界の直前の CRLF）を除去。
        let content = strip_trailing_crlf(content);

        let part = parse_part(header_text, content)?;
        parts.push(part);

        // 次の境界の先頭へ移動する。
        pos = cur + next;
    }

    Ok(parts)
}

/// ヘッダ文とボディからパートを構成する。
fn parse_part(header_text: &str, content: Vec<u8>) -> Result<Part, MultipartError> {
    let mut name: Option<String> = None;
    let mut filename: Option<String> = None;

    for line in header_text.split("\r\n") {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line
            .to_ascii_lowercase()
            .strip_prefix("content-disposition:")
        {
            for token in split_disposition(value) {
                if let Some(rest) = token.trim().strip_prefix("name=") {
                    name = Some(unquote(rest.trim()));
                } else if let Some(rest) = token.trim().strip_prefix("filename=") {
                    filename = Some(unquote(rest.trim()));
                }
            }
        }
    }

    let name = name.ok_or_else(|| MultipartError::InvalidBody("Content-Disposition の name が無い".into()))?;
    Ok(Part { name, filename, content })
}

/// `Content-Disposition` の値を `;` で分割（引用符内の `;` を避ける簡易実装）。
fn split_disposition(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in value.chars() {
        if ch == '"' {
            in_quote = !in_quote;
            current.push(ch);
        } else if ch == ';' && !in_quote {
            out.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// 引用符を外す。
fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// 末尾の CRLF を除去。
fn strip_trailing_crlf(mut content: Vec<u8>) -> Vec<u8> {
    if content.ends_with(b"\r\n") {
        content.pop();
        content.pop();
    }
    content
}

/// サブスライスを先頭から探す。
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// multipart 本体を構築するテスト補助。
    fn build(boundary: &str, parts: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (name, filename, content) in parts {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            if let Some(fname) = filename {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{fname}\"\r\n")
                        .as_bytes(),
                );
                body.extend_from_slice(b"Content-Type: application/zip\r\n");
            } else {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n").as_bytes(),
                );
            }
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(content);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        body
    }

    #[test]
    fn extracts_boundary_from_content_type() {
        assert_eq!(
            boundary_from_content_type("multipart/form-data; boundary=----abc").unwrap(),
            "----abc"
        );
        assert_eq!(
            boundary_from_content_type("multipart/form-data; boundary=\"abc\"").unwrap(),
            "abc"
        );
        assert!(boundary_from_content_type("application/json").is_err());
        assert!(boundary_from_content_type("multipart/form-data").is_err());
    }

    #[test]
    fn parses_text_and_file_parts() {
        let boundary = "----Xboundary123";
        let title_bytes = "月明かりの庭".as_bytes();
        let body = build(
            boundary,
            &[
                ("title", None, title_bytes),
                ("archive", Some("book.zip"), b"PK\x03\x04zipdata"),
            ],
        );
        let parts = parse(
            &format!("multipart/form-data; boundary={boundary}"),
            &body,
        )
        .expect("パース成功");

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "title");
        assert_eq!(parts[0].text(), "月明かりの庭");
        assert!(parts[0].filename.is_none());

        assert_eq!(parts[1].name, "archive");
        assert_eq!(parts[1].filename.as_deref(), Some("book.zip"));
        assert_eq!(parts[1].content, b"PK\x03\x04zipdata");
    }

    #[test]
    fn handles_binary_content_with_crlf() {
        let boundary = "b";
        // ボディに CRLF を含んでも正しく分割される。
        let body = build(boundary, &[("a", None, b"line1\r\nline2")]);
        let parts = parse(&format!("multipart/form-data; boundary={boundary}"), &body).unwrap();
        assert_eq!(parts[0].text(), "line1\r\nline2");
    }

    #[test]
    fn missing_boundary_errors() {
        let err = parse("application/json", b"{}").unwrap_err();
        assert!(matches!(err, MultipartError::MissingBoundary));
    }

    #[test]
    fn invalid_body_without_delimiter_errors() {
        let err = parse("multipart/form-data; boundary=x", b"no delimiter here").unwrap_err();
        assert!(matches!(err, MultipartError::InvalidBody(_)));
    }
}
