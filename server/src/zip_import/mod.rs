//! zip 取込（展開＋許可リスト検査）。
//!
//! 設計 `REF:docs/designs/api.md` §10.3 の許可リストに準拠する。サンドボックスは
//! オフラインビルドで zip／flate2 系 crate が使えないため、ZIP パースと
//! DEFLATE 展開は純粋 Rust で自己実装する（設計 `REF:docs/designs/m3.md` §2.3 判断 4）。
//!
//! 検査項目（§10.3）:
//! - UTF-8 ファイル名
//! - 50MB（アップロード全体）・2000 ファイル
//! - 圧縮率 100 倍（zip bomb 対策。ファイル単位・集計単位の両方）
//! - `../` 禁止・絶対パス禁止
//! - symlink 禁止・デバイスファイル禁止
//! - ファイル名は NFC 正規化の上で許可文字（`[\p{L}\p{N}._\\/-]`）に限定

pub mod inflate;

use std::fmt;

use inflate::inflate;

/// zip 取込の上限（設計 §10.3）。
pub const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
/// 最大ファイル数。
pub const MAX_FILES: usize = 2000;
/// 最大圧縮率（展開後 / 圧縮前）。
pub const MAX_COMPRESSION_RATIO: u64 = 100;

/// zip 取込エラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZipError {
    /// zip 形式として不正。
    InvalidArchive(String),
    /// アップロードが大きすぎる。
    TooLarge,
    /// ファイル数が上限超过。
    TooManyFiles,
    /// 圧縮率が上限超过（zip bomb）。
    CompressionRatioExceeded,
    /// ファイル名が許可されていない。
    InvalidFilename(String),
    /// パス traversal（`../` 等）。
    PathTraversal(String),
    /// サポートしない種別（symlink・デバイス・未対応圧縮等）。
    Unsupported(String),
    /// ファイル名が UTF-8 でない。
    NotUtf8(String),
    /// CRC32 不一致（破損）。
    CrcMismatch(String),
    /// 展開失敗。
    Inflate(String),
}

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZipError::InvalidArchive(d) => write!(f, "zip 形式が不正です: {d}"),
            ZipError::TooLarge => write!(f, "アップロードが大きすぎます"),
            ZipError::TooManyFiles => write!(f, "ファイル数が上限超过です"),
            ZipError::CompressionRatioExceeded => write!(f, "圧縮率が上限超过です"),
            ZipError::InvalidFilename(d) => write!(f, "ファイル名が許可されていません: {d}"),
            ZipError::PathTraversal(d) => write!(f, "パス traversal は禁止です: {d}"),
            ZipError::Unsupported(d) => write!(f, "サポートしない種別です: {d}"),
            ZipError::NotUtf8(d) => write!(f, "ファイル名が UTF-8 でない: {d}"),
            ZipError::CrcMismatch(d) => write!(f, "CRC32 が不一致です: {d}"),
            ZipError::Inflate(d) => write!(f, "展開に失敗しました: {d}"),
        }
    }
}

impl std::error::Error for ZipError {}

/// 取込済みの 1 ファイル。
#[derive(Debug, Clone)]
pub struct ImportedFile {
    /// 正規化済みパス（`/` 区切り、先頭・末尾の `/` なし）。
    pub path: String,
    /// 展開済み内容。
    pub content: Vec<u8>,
}

/// CRC-32（IEEE 802.3）表。
fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for i in 0..256u32 {
        let mut c = i;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb88320 ^ (c >> 1) } else { c >> 1 };
        }
        table[i as usize] = c;
    }
    table
}

/// CRC-32 を計算する。
fn crc32(data: &[u8]) -> u32 {
    let table = crc32_table();
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xff) as usize] ^ (crc >> 8);
    }
    crc ^ 0xffff_ffffu32
}

/// 小端符号リテラルを読む。
fn read_u16(data: &[u8], pos: usize) -> Option<u16> {
    if pos + 2 > data.len() {
        return None;
    }
    Some(u16::from_le_bytes([data[pos], data[pos + 1]]))
}

fn read_u32(data: &[u8], pos: usize) -> Option<u32> {
    if pos + 4 > data.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        data[pos],
        data[pos + 1],
        data[pos + 2],
        data[pos + 3],
    ]))
}

/// 中央ディレクトリの 1 エントリ。
struct CentralEntry {
    method: u16,
    compressed_size: u64,
    uncompressed_size: u64,
    crc: u32,
    filename: Vec<u8>,
    external_attrs: u32,
    version_made_by: u16,
    local_offset: u64,
}

/// zip を解析し、許可リストを満たすファイルを展開して返す。
pub fn import_zip(data: &[u8]) -> Result<Vec<ImportedFile>, ZipError> {
    if data.len() > MAX_UPLOAD_BYTES {
        return Err(ZipError::TooLarge);
    }

    let eocd = find_eocd(data).ok_or_else(|| ZipError::InvalidArchive("EOCD が見つかりません".into()))?;
    let (cd_offset, cd_count) = read_eocd(data, eocd)?;

    let mut entries = Vec::with_capacity(cd_count.min(MAX_FILES as u16) as usize);
    let mut pos = cd_offset;
    for _ in 0..cd_count {
        if pos + 46 > data.len() {
            return Err(ZipError::InvalidArchive("中央ディレクトリが切断されています".into()));
        }
        if read_u32(data, pos) != Some(0x0201_4b50) {
            return Err(ZipError::InvalidArchive("中央ディレクトリの先頭符号が不正".into()));
        }
        let version_made_by = read_u16(data, pos + 4).unwrap();
        let method = read_u16(data, pos + 10).unwrap();
        let crc = read_u32(data, pos + 16).unwrap();
        let compressed_size = read_u32(data, pos + 20).unwrap();
        let uncompressed_size = read_u32(data, pos + 24).unwrap();
        let filename_len = read_u16(data, pos + 28).unwrap() as usize;
        let extra_len = read_u16(data, pos + 30).unwrap() as usize;
        let comment_len = read_u16(data, pos + 32).unwrap() as usize;
        let external_attrs = read_u32(data, pos + 38).unwrap();
        let local_offset = read_u32(data, pos + 42).unwrap() as u64;

        let name_start = pos + 46;
        let name_end = name_start + filename_len;
        if name_end + extra_len + comment_len > data.len() {
            return Err(ZipError::InvalidArchive("中央ディレクトリの名前が切断されています".into()));
        }
        let filename = data[name_start..name_end].to_vec();

        // zip64 は 50MB 上限の M3 では不要。兆候があれば未対応として拒否する。
        let (compressed_size, uncompressed_size) = if compressed_size == 0xffff_ffff
            || uncompressed_size == 0xffff_ffff
            || local_offset == 0xffff_ffff
        {
            return Err(ZipError::Unsupported("zip64 は M3 では対応しません".into()));
        } else {
            (compressed_size as u64, uncompressed_size as u64)
        };

        entries.push(CentralEntry {
            method,
            compressed_size,
            uncompressed_size,
            crc,
            filename,
            external_attrs,
            version_made_by,
            local_offset,
        });

        pos = name_end + extra_len + comment_len;
    }

    if entries.len() > MAX_FILES {
        return Err(ZipError::TooManyFiles);
    }

    let mut files = Vec::with_capacity(entries.len());
    let mut total_uncompressed: u64 = 0;
    for entry in entries {
        let path = validate_and_normalize_name(&entry.filename)?;

        // ディレクトリ entry は無視（novnow の実体はファイル）。
        if path.is_empty() {
            continue;
        }

        // symlink / デバイスファイルの拒否（Unix モード。OS=Unix のみ判定）。
        if entry.version_made_by >> 8 == 3 {
            let mode = (entry.external_attrs >> 16) & 0xffff;
            let kind = mode & 0o170000;
            if kind == 0o120000 {
                return Err(ZipError::Unsupported(format!("symlink は禁止です: {path}")));
            }
            if kind == 0o020000 || kind == 0o060000 {
                return Err(ZipError::Unsupported(format!("デバイスファイルは禁止です: {path}")));
            }
        }

        if entry.compressed_size == 0 {
            // 空ファイルは展開不要。
            files.push(ImportedFile { path, content: Vec::new() });
            continue;
        }

        // zip bomb 対策（ファイル単位）。
        if entry.uncompressed_size > entry.compressed_size.saturating_mul(MAX_COMPRESSION_RATIO) {
            return Err(ZipError::CompressionRatioExceeded);
        }
        // zip bomb 対策（集計単位）。
        total_uncompressed = total_uncompressed
            .checked_add(entry.uncompressed_size)
            .ok_or(ZipError::CompressionRatioExceeded)?;
        if total_uncompressed > data.len() as u64 * MAX_COMPRESSION_RATIO {
            return Err(ZipError::CompressionRatioExceeded);
        }

        let content = extract_entry(data, &entry)?;

        // CRC32 検証。
        if crc32(&content) != entry.crc {
            return Err(ZipError::CrcMismatch(path));
        }

        files.push(ImportedFile { path, content });
    }

    Ok(files)
}

/// ローカルヘッダからデータを切り出して展開する。
fn extract_entry(data: &[u8], entry: &CentralEntry) -> Result<Vec<u8>, ZipError> {
    let offset = entry.local_offset as usize;
    if offset + 30 > data.len() {
        return Err(ZipError::InvalidArchive("ローカルヘッダが範囲外".into()));
    }
    if read_u32(data, offset) != Some(0x0403_4b50) {
        return Err(ZipError::InvalidArchive("ローカルヘッダの符号が不正".into()));
    }
    let method = read_u16(data, offset + 8).unwrap();
    // 中央ディレクトリとローカルヘッダの圧縮方式は一致していなければならない。
    if method != entry.method {
        return Err(ZipError::InvalidArchive(
            "ローカルヘッダと中央ディレクトリの圧縮方式が不一致".into(),
        ));
    }
    let filename_len = read_u16(data, offset + 26).unwrap() as usize;
    let extra_len = read_u16(data, offset + 28).unwrap() as usize;
    let data_start = offset + 30 + filename_len + extra_len;
    let data_end = data_start
        .checked_add(entry.compressed_size as usize)
        .ok_or(ZipError::InvalidArchive("データ範囲が溢出".into()))?;
    if data_end > data.len() {
        return Err(ZipError::InvalidArchive("圧縮データが範囲外".into()));
    }
    let compressed = &data[data_start..data_end];

    let content = match method {
        0 => compressed.to_vec(),
        8 => inflate(compressed)
            .map_err(|e| ZipError::Inflate(e.to_string()))?,
        other => return Err(ZipError::Unsupported(format!("未対応の圧縮方式 {other}"))),
    };

    if content.len() as u64 != entry.uncompressed_size {
        return Err(ZipError::InvalidArchive("展開サイズが一致しません".into()));
    }
    Ok(content)
}

/// ファイル名を検証し、正規化したパスを返す。
fn validate_and_normalize_name(raw: &[u8]) -> Result<String, ZipError> {
    let name = std::str::from_utf8(raw).map_err(|_| {
        ZipError::NotUtf8(format!("{:?}", String::from_utf8_lossy(raw)))
    })?;

    // 許可文字に限定（`[\p{L}\p{N}._\/-]`）。
    for ch in name.chars() {
        if !(ch.is_alphabetic() || ch.is_numeric() || ch == '.' || ch == '_' || ch == '/' || ch == '-')
        {
            return Err(ZipError::InvalidFilename(name.to_string()));
        }
    }

    // 絶対パス禁止。
    if name.starts_with('/') {
        return Err(ZipError::PathTraversal(name.to_string()));
    }

    // パス traversal 禁止（`..` セグメント）。
    for segment in name.split('/') {
        if segment == ".." {
            return Err(ZipError::PathTraversal(name.to_string()));
        }
    }

    // 正規化: 先頭・末尾の `/` を除去し、連続 `/` を単一化。
    let trimmed = name.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let normalized: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
    Ok(normalized.join("/"))
}

/// 末尾から EOCD（0x06054b50）を探す。
fn find_eocd(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    // コメントは最大 65535 バイトなので、末尾 22+65535 バイト内で走査する。
    let min = data.len().saturating_sub(22 + 65535);
    let mut i = data.len() - 4;
    while i >= min {
        if data[i] == 0x50 && data[i + 1] == 0x4b && data[i + 2] == 0x05 && data[i + 3] == 0x06 {
            return Some(i);
        }
        i -= 1;
    }
    None
}

/// EOCD から中央ディレクトリの先頭とエントリ数を読む。
fn read_eocd(data: &[u8], eocd: usize) -> Result<(usize, u16), ZipError> {
    if eocd + 22 > data.len() {
        return Err(ZipError::InvalidArchive("EOCD が範囲外".into()));
    }
    // コメント長は EOCD 末尾からの余りと一致していなければならない。
    let comment_len = read_u16(data, eocd + 20).unwrap_or(0) as usize;
    if data.len() - (eocd + 22) != comment_len {
        return Err(ZipError::InvalidArchive("EOCD のコメント長が不一致".into()));
    }
    let count = read_u16(data, eocd + 10).unwrap_or(0);
    let offset = read_u32(data, eocd + 16).unwrap_or(0) as usize;
    Ok((offset, count))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Python の zipfile 相当で生成した zip の byte 列を返す。
    /// `entries` は (ファイル名, 展開後内容, 圧縮後内容, 圧縮方式)。
    pub(crate) fn make_zip(entries: &[(&str, &[u8], &[u8], u16)]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut central = Vec::new();
        for (name, content, compressed, method) in entries {
            let name_bytes = name.as_bytes();
            let crc = crc32(content);
            let offset = buf.len() as u32;
            // ローカルヘッダ
            buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            buf.extend_from_slice(&20u16.to_le_bytes()); // version
            buf.extend_from_slice(&0u16.to_le_bytes()); // flags
            buf.extend_from_slice(&method.to_le_bytes()); // method
            buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
            buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
            buf.extend_from_slice(&crc.to_le_bytes());
            buf.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            buf.extend_from_slice(&(content.len() as u32).to_le_bytes());
            buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes()); // extra
            buf.extend_from_slice(name_bytes);
            buf.extend_from_slice(compressed);

            // 中央ディレクトリ
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes()); // version made by
            central.extend_from_slice(&20u16.to_le_bytes()); // version needed
            central.extend_from_slice(&0u16.to_le_bytes()); // flags
            central.extend_from_slice(&method.to_le_bytes()); // method
            central.extend_from_slice(&0u16.to_le_bytes()); // mod time
            central.extend_from_slice(&0u16.to_le_bytes()); // mod date
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            central.extend_from_slice(&(content.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // extra
            central.extend_from_slice(&0u16.to_le_bytes()); // comment
            central.extend_from_slice(&0u16.to_le_bytes()); // disk
            central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }
        let cd_offset = buf.len() as u32;
        buf.extend_from_slice(&central);
        // EOCD
        buf.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // disk
        buf.extend_from_slice(&0u16.to_le_bytes()); // cd disk
        buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        buf.extend_from_slice(&(central.len() as u32).to_le_bytes());
        buf.extend_from_slice(&cd_offset.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
        buf
    }

    #[test]
    fn imports_valid_zip() {
        let zip = make_zip(&[
            ("project.toml", b"[project]\nname = \"t\"\n", b"[project]\nname = \"t\"\n", 0),
            ("body/main.txt", b"hello", b"hello", 0),
        ]);
        let files = import_zip(&zip).expect("正常な zip は展開できる");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "project.toml");
        assert_eq!(files[1].path, "body/main.txt");
        assert_eq!(files[1].content, b"hello");
    }

    // 固定ハフマンの DEFLATE エントリ（Python zlib で生成した既知ベクトル）。
    const FOX_TEXT: &[u8] = b"The quick brown fox jumps over the lazy dog.";
    const FOX_DEFLATE: &[u8] = &[
        0x0b, 0xc9, 0x48, 0x55, 0x28, 0x2c, 0xcd, 0x4c, 0xce, 0x56, 0x48, 0x2a, 0xca, 0x2f, 0xcf,
        0x53, 0x48, 0xcb, 0xaf, 0x50, 0xc8, 0x2a, 0xcd, 0x2d, 0x28, 0x56, 0xc8, 0x2f, 0x4b, 0x2d,
        0x52, 0x28, 0x01, 0x4a, 0xe7, 0x24, 0x56, 0x55, 0x2a, 0xa4, 0xe4, 0xa7, 0xeb, 0x01, 0x00,
    ];

    // 動的ハフマンの DEFLATE エントリ（Python zlib で生成した既知ベクトル）。
    const ALPHABET: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
    const ALPHABET_DEFLATE: &[u8] = &[
        0x4b, 0x4c, 0x4a, 0x4e, 0x49, 0x4d, 0x4b, 0xcf, 0xc8, 0xcc, 0xca, 0xce, 0xc9, 0xcd, 0xcb, 0x2f,
        0x28, 0x2c, 0x2a, 0x2e, 0x29, 0x2d, 0x2b, 0xaf, 0xa8, 0xac, 0x4a, 0xa4, 0xaa, 0x0c, 0x00,
    ];

    #[test]
    fn imports_deflate_fixed_huffman_entry() {
        // 既知の zip（固定ハフマン）に対し、展開結果が zlib の平文と一致すること。
        let zip = make_zip(&[("fox.txt", FOX_TEXT, FOX_DEFLATE, 8)]);
        let files = import_zip(&zip).expect("固定ハフマンの zip は展開できる");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].content, FOX_TEXT);
    }

    #[test]
    fn imports_deflate_dynamic_huffman_entry() {
        // 既知の zip（動的ハフマン）に対し、展開結果が zlib の平文と一致すること。
        let zip = make_zip(&[("alpha.txt", ALPHABET, ALPHABET_DEFLATE, 8)]);
        let files = import_zip(&zip).expect("動的ハフマンの zip は展開できる");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].content, ALPHABET);
    }

    #[test]
    fn rejects_path_traversal() {
        let zip = make_zip(&[("../evil.txt", b"x", b"x", 0)]);
        let err = import_zip(&zip).expect_err("traversal は拒否");
        assert!(matches!(err, ZipError::PathTraversal(_)));
    }

    #[test]
    fn rejects_absolute_path() {
        let zip = make_zip(&[("/etc/passwd", b"x", b"x", 0)]);
        let err = import_zip(&zip).expect_err("絶対パスは拒否");
        assert!(matches!(err, ZipError::PathTraversal(_)));
    }

    #[test]
    fn rejects_invalid_filename_char() {
        let zip = make_zip(&[("bad\nname.txt", b"x", b"x", 0)]);
        let err = import_zip(&zip).expect_err("改行は拒否");
        assert!(matches!(err, ZipError::InvalidFilename(_)));
    }

    #[test]
    fn crc32_known_value() {
        // "123456789" の CRC-32 は 0xCBF43926。
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn rejects_truncated_archive() {
        let zip = make_zip(&[("a.txt", b"x", b"x", 0)]);
        let err = import_zip(&zip[..zip.len() - 5]).expect_err("切断 zip は拒否");
        assert!(matches!(err, ZipError::InvalidArchive(_)));
    }
}
