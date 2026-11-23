//! GitHub アダプターの最小実装。
//!
//! 設計書 `REF:docs/designs/api.md` §7 に準拠する。GitHub へのアクセスはすべて
//! このモジュール内に閉じ込め、`ports` のトレイト越しにのみ使われる。
//! GitHub の型（Repository 等）は `pub` に出さない。

pub mod client;

use std::sync::Arc;

use crate::ports::{
    ContentPort, FileContent, Identity, IdentityPort, NewFile, PortError, ProjectShell,
    ProjectStoragePort, ProjectVisibility, TreeEntry, TreeEntryKind,
};

use self::client::{GitHubClient, OAuthConfig, Token};

/// GitHub アダプター。`IdentityPort`／`ProjectStoragePort`／`ContentPort` を実装する。
///
/// 認証トークンはアダプターが保持し、各リクエストに付与する。トークンの
/// 取得は `client::GitHubClient` に委譲する（設計 §7.4 の暗号化トークン
/// ストアは後続タスク）。トークンは OAuth 交換後に `set_token` で設定する。
pub struct GitHubAdapter {
    client: GitHubClient,
    token: Arc<std::sync::RwLock<Option<Token>>>,
    oauth: OAuthConfig,
}

impl GitHubAdapter {
    pub fn new(client: GitHubClient, oauth: OAuthConfig) -> Self {
        Self {
            client,
            token: Arc::new(std::sync::RwLock::new(None)),
            oauth,
        }
    }

    /// OAuth 交換で得られたアクセストークンを設定する。
    pub fn set_token(&self, token: Token) {
        *self.token.write().unwrap() = Some(token);
    }

    fn token(&self) -> Result<Token, PortError> {
        self.token
            .read()
            .unwrap()
            .clone()
            .ok_or(PortError::PermissionDenied)
    }
}

impl IdentityPort for GitHubAdapter {
    async fn exchange_code_for_identity(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<Identity, PortError> {
        // 認可コードを GitHub のアクセストークンへ交換（PKCE）。
        let token = self
            .client
            .exchange_code(code, code_verifier, &self.oauth)
            .await
            .map_err(|e| map_client_error(&e))?;

        // トークンで利用者情報を取得。
        let user = self
            .client
            .get_user(&token)
            .await
            .map_err(|e| map_client_error(&e))?;

        // 以降の保管単位操作に使うトークンとして保持。
        self.set_token(token);

        Ok(Identity {
            id: user.id.to_string(),
            handle: user.login.clone(),
            display_name: user.name.unwrap_or_else(|| user.login.clone()),
            avatar_url: Some(user.avatar_url),
        })
    }

    async fn revoke_grant(&self, _token: &str) -> Result<(), PortError> {
        // 最小実装: GitHub 側の許可取消しは後続タスク。ここでは成功を返す。
        Ok(())
    }
}

impl ProjectStoragePort for GitHubAdapter {
    async fn create_project_shell(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<ProjectShell, PortError> {
        let token = self.token()?;
        let repo = self
            .client
            .create_repo(&token, name, description)
            .await
            .map_err(|e| map_client_error(&e))?;
        let visibility = if repo.private {
            ProjectVisibility::Private
        } else {
            ProjectVisibility::Public
        };
        Ok(ProjectShell {
            id: repo.full_name,
            default_branch: repo.default_branch,
            visibility,
            etag: repo.etag.unwrap_or_default(),
        })
    }

    async fn get_project_meta(&self, shell_id: &str) -> Result<ProjectShell, PortError> {
        let token = self.token()?;
        let repo = self
            .client
            .get_repo(&token, shell_id)
            .await
            .map_err(|e| map_client_error(&e))?;
        let visibility = if repo.private {
            ProjectVisibility::Private
        } else {
            ProjectVisibility::Public
        };
        Ok(ProjectShell {
            id: repo.full_name,
            default_branch: repo.default_branch,
            visibility,
            etag: repo.etag.unwrap_or_default(),
        })
    }

    async fn set_visibility(
        &self,
        shell_id: &str,
        visibility: ProjectVisibility,
    ) -> Result<(), PortError> {
        let token = self.token()?;
        let private = matches!(visibility, ProjectVisibility::Private);
        self.client
            .set_repo_private(&token, shell_id, private)
            .await
            .map_err(|e| map_client_error(&e))
    }

    async fn delete_project_shell(&self, shell_id: &str) -> Result<(), PortError> {
        let token = self.token()?;
        self.client
            .delete_repo(&token, shell_id)
            .await
            .map_err(|e| map_client_error(&e))
    }
}

impl ContentPort for GitHubAdapter {
    async fn list_tree(
        &self,
        shell_id: &str,
        path: Option<&str>,
    ) -> Result<Vec<TreeEntry>, PortError> {
        let token = self.token()?;
        let entries = self
            .client
            .list_contents(&token, shell_id, path)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(entries
            .into_iter()
            .map(|e| TreeEntry {
                path: e.path,
                kind: match e.entry_type.as_str() {
                    "dir" => TreeEntryKind::Directory,
                    _ => TreeEntryKind::File,
                },
                size: e.size,
                etag: e.sha,
            })
            .collect())
    }

    async fn read_file(&self, shell_id: &str, path: &str) -> Result<FileContent, PortError> {
        let token = self.token()?;
        let file = self
            .client
            .get_file(&token, shell_id, path)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(FileContent {
            content: file.content,
            encoding: match file.encoding {
                client::FileEncoding::Text => crate::ports::FileEncoding::Text,
                client::FileEncoding::Base64 => crate::ports::FileEncoding::Base64,
            },
            etag: file.sha,
        })
    }

    async fn commit_files(
        &self,
        shell_id: &str,
        branch: &str,
        files: &[NewFile],
    ) -> Result<String, PortError> {
        let token = self.token()?;
        self.client
            .commit_files(&token, shell_id, branch, files)
            .await
            .map_err(|e| map_client_error(&e))
    }

    async fn get_etag(&self, shell_id: &str, path: Option<&str>) -> Result<String, PortError> {
        let token = self.token()?;
        match path {
            Some(p) => {
                let file = self
                    .client
                    .get_file(&token, shell_id, p)
                    .await
                    .map_err(|e| map_client_error(&e))?;
                Ok(file.sha)
            }
            None => {
                let repo = self
                    .client
                    .get_repo(&token, shell_id)
                    .await
                    .map_err(|e| map_client_error(&e))?;
                Ok(repo.etag.unwrap_or_default())
            }
        }
    }
}

/// クライアントエラーから PortError への変換（設計 §7.3 の表に準拠）。
fn map_client_error(e: &client::ClientError) -> PortError {
    match e {
        client::ClientError::NotFound => PortError::NotFound,
        client::ClientError::Conflict => PortError::Conflict,
        client::ClientError::PermissionDenied => PortError::PermissionDenied,
        client::ClientError::RateLimited => PortError::RateLimited,
        client::ClientError::Unavailable(_) => PortError::Unavailable,
        client::ClientError::InvalidResponse(_) => PortError::InvalidResponse,
        client::ClientError::Other(_) => PortError::Unknown,
    }
}

/// アダプターを共有ハンドルで包む。
pub type SharedGitHubAdapter = Arc<GitHubAdapter>;
