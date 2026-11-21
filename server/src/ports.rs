//! ポート（trait）と hub ドメイン型の定義。
//!
//! 設計書 `REF:docs/designs/api.md` §7 に準拠する。ユースケース層はここに
//! 定義された語彙のみを知り、GitHub の概念（Repository 等）は一切知らない。
//! 依存方向は `api → usecase → ports ← adapters`。

use std::fmt;

/// 外部基盤のエラー。仕様書 §7.3 の 7 値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortError {
    /// 対象が存在しない。
    NotFound,
    /// 競合（版のずれ等）。
    Conflict,
    /// 権限不足。
    PermissionDenied,
    /// レートリミット。
    RateLimited,
    /// 外部基盤が利用できない（タイムアウト・5xx 等）。
    Unavailable,
    /// 外部基盤の応答が想定と乖離している。
    InvalidResponse,
    /// その他の未知のエラー。
    Unknown,
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PortError::NotFound => "not_found",
            PortError::Conflict => "conflict",
            PortError::PermissionDenied => "permission_denied",
            PortError::RateLimited => "rate_limited",
            PortError::Unavailable => "unavailable",
            PortError::InvalidResponse => "invalid_response",
            PortError::Unknown => "unknown",
        };
        write!(f, "{s}")
    }
}

impl std::error::Error for PortError {}

/// hub の利用者像（GitHub の概念を含まない）。
#[derive(Debug, Clone)]
pub struct Identity {
    /// 不透明な利用者識別子。
    pub id: String,
    /// ハンドル（@ 付きで表示する名前の本体）。
    pub handle: String,
    /// 表示名。
    pub display_name: String,
    /// アバター URL（任意）。
    pub avatar_url: Option<String>,
}

/// 保管単位の公開範囲（hub 語彙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectVisibility {
    Public,
    Private,
    Archived,
}

/// 保管単位（リポジトリ相当）。
#[derive(Debug, Clone)]
pub struct ProjectShell {
    /// 保管単位の識別子（不透明文字列）。
    pub id: String,
    /// 既定分岐名。
    pub default_branch: String,
    /// 公開範囲。
    pub visibility: ProjectVisibility,
    /// 版識別子（ETag）。
    pub etag: String,
}

/// ツリー要素の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeEntryKind {
    File,
    Directory,
}

/// ツリーの要素 1 件。
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: String,
    pub kind: TreeEntryKind,
    pub size: Option<u64>,
    /// blob 単位の版識別子。
    pub etag: String,
}

/// ファイルの符号化方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEncoding {
    /// UTF-8 テキスト。
    Text,
    /// Base64（バイナリ相当）。
    Base64,
}

/// ファイル内容と版識別子。
#[derive(Debug, Clone)]
pub struct FileContent {
    pub content: String,
    pub encoding: FileEncoding,
    pub etag: String,
}

/// コミット対象の新規ファイル。
#[derive(Debug, Clone)]
pub struct NewFile {
    pub path: String,
    pub content: String,
}

/// 外部サービスの利用者像とアクセス権への交換。
pub trait IdentityPort: Send + Sync {
    /// 認可コードを外部サービスの利用者像へ交換する（PKCE の code_verifier を渡す）。
    fn exchange_code_for_identity(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> impl std::future::Future<Output = Result<Identity, PortError>> + Send;

    /// OAuth 許可を失効させる。
    fn revoke_grant(
        &self,
        token: &str,
    ) -> impl std::future::Future<Output = Result<(), PortError>> + Send;
}

/// 保管単位の作成・参照・公開範囲切り替え。
pub trait ProjectStoragePort: Send + Sync {
    /// 空の保管単位を作成する。
    fn create_project_shell(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> impl std::future::Future<Output = Result<ProjectShell, PortError>> + Send;

    /// 保管単位のメタ（公開範囲・既定分岐・ETag）を取得する。
    fn get_project_meta(
        &self,
        shell_id: &str,
    ) -> impl std::future::Future<Output = Result<ProjectShell, PortError>> + Send;

    /// 公開／非公開を切り替える。
    fn set_visibility(
        &self,
        shell_id: &str,
        visibility: ProjectVisibility,
    ) -> impl std::future::Future<Output = Result<(), PortError>> + Send;

    /// 保管単位を取り下げる。
    fn delete_project_shell(
        &self,
        shell_id: &str,
    ) -> impl std::future::Future<Output = Result<(), PortError>> + Send;
}

/// コンテンツ（ツリー・ファイル・コミット）の操作。
pub trait ContentPort: Send + Sync {
    /// ファイルツリーを取得する。
    fn list_tree(
        &self,
        shell_id: &str,
        path: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Vec<TreeEntry>, PortError>> + Send;

    /// ファイル内容と ETag を取得する。
    fn read_file(
        &self,
        shell_id: &str,
        path: &str,
    ) -> impl std::future::Future<Output = Result<FileContent, PortError>> + Send;

    /// 複数ファイルの原子的コミットを作成する。返り値はコミット識別子。
    fn commit_files(
        &self,
        shell_id: &str,
        branch: &str,
        files: &[NewFile],
    ) -> impl std::future::Future<Output = Result<String, PortError>> + Send;

    /// ファイル／プロジェクトの版識別子を取得する。
    fn get_etag(
        &self,
        shell_id: &str,
        path: Option<&str>,
    ) -> impl std::future::Future<Output = Result<String, PortError>> + Send;
}
