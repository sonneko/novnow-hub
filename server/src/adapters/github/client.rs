//! GitHub REST API の薄い reqwest ラッパ。
//!
//! 設計書 `REF:docs/designs/api.md` §7 に準拠する。GitHub SDK は使わず、
//! 必要な操作のみを最小で実装する。エラーは `ClientError` に正規化し、
//! アダプター側で `PortError` へ変換する。

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;

use crate::ports::NewFile;

/// クライアントのエラー。
#[derive(Debug)]
pub enum ClientError {
    NotFound,
    Conflict,
    PermissionDenied,
    RateLimited,
    Unavailable(String),
    InvalidResponse(String),
    Other(String),
}

/// GitHub アクセストークン（暗号化ストアは後続タスク。最小実装では保持のみ）。
#[derive(Debug, Clone)]
pub struct Token(String);

impl Token {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// GitHub API クライアント。
pub struct GitHubClient {
    inner: reqwest::Client,
    base_url: String,
    timeout_read: Duration,
    timeout_write: Duration,
}

impl GitHubClient {
    pub fn new(base_url: &str, timeout_read: Duration, timeout_write: Duration) -> Self {
        let inner = reqwest::Client::builder()
            .timeout(timeout_write)
            .build()
            .expect("reqwest クライアントの初期化に失敗");
        Self {
            inner,
            base_url: base_url.trim_end_matches('/').to_string(),
            timeout_read,
            timeout_write,
        }
    }

    fn headers(&self, token: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("novnow-hub-server"),
        );
        if let Some(t) = token {
            let mut value = HeaderValue::from_str(&format!("Bearer {t}"))
                .expect("トークンに制御文字が含まれている");
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        headers
    }

    fn map_status(status: reqwest::StatusCode) -> ClientError {
        match status.as_u16() {
            404 => ClientError::NotFound,
            409 => ClientError::Conflict,
            403 => ClientError::PermissionDenied,
            429 => ClientError::RateLimited,
            502 | 503 | 504 => ClientError::Unavailable("上流障害".into()),
            other => ClientError::Other(format!("HTTP {other}")),
        }
    }

    /// 429 の再試行（`Retry-After` 尊重、上限 3 回）。設計 §7 の共通規約。
    ///
    /// `RequestBuilder` は `send()` でムーブされるため再利用できない。
    /// 呼び出し側はリクエストを構築するクローズを渡し、各試行で再構築する。
    async fn send_with_retry(
        &self,
        mut build: impl FnMut() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ClientError> {
        for attempt in 0..3 {
            let request = build();
            let response = request.send().await.map_err(|e| {
                ClientError::Unavailable(format!("リクエスト失敗: {e}"))
            })?;
            let status = response.status();
            if status.as_u16() != 429 || attempt == 2 {
                return Ok(response);
            }
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1);
            tokio::time::sleep(Duration::from_secs(retry_after)).await;
        }
        unreachable!()
    }

    /// PKCE の code_challenge 計算（S256）。
    pub fn pkce_challenge(code_verifier: &str) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(code_verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    }

    /// 認可コードをアクセストークンへ交換（PKCE）。
    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
        oauth: &OAuthConfig,
    ) -> Result<Token, ClientError> {
        let body = serde_json::json!({
            "client_id": oauth.client_id,
            "code": code,
            "code_verifier": code_verifier,
        });
        let response = self
            .send_with_retry(|| {
                self.inner
                    .post(format!("{}/login/oauth/access_token", self.base_url))
                    .json(&body)
            })
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Self::map_status(status));
        }
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))?;
        let token_value = payload
            .get("access_token")
            .and_then(|t| t.as_str())
            .ok_or_else(|| ClientError::InvalidResponse("access_token が無い".into()))?;
        Ok(Token::new(token_value.to_string()))
    }

    /// ログイン中の利用者を取得。
    pub async fn get_user(&self, token: &Token) -> Result<GitHubUser, ClientError> {
        let response = self
            .send_with_retry(|| {

                self.inner
                    .get(format!("{}/user", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<GitHubUser>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// 保管単位（リポジトリ）を作成。
    pub async fn create_repo(
        &self,
        token: &Token,
        name: &str,
        description: Option<&str>,
    ) -> Result<RepoMeta, ClientError> {
        let mut body = serde_json::json!({ "name": name });
        if let Some(d) = description {
            body["description"] = serde_json::json!(d);
        }
        let response = self
            .send_with_retry(|| {

                self.inner
                    .post(format!("{}/user/repos", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(&body)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<RepoMeta>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// 保管単位のメタを取得。
    pub async fn get_repo(&self, token: &Token, full_name: &str) -> Result<RepoMeta, ClientError> {
        let response = self
            .send_with_retry(|| {

                self.inner
                    .get(format!("{}/repos/{full_name}", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<RepoMeta>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// 保管単位の公開／非公開を切り替え。
    pub async fn set_repo_private(
        &self,
        token: &Token,
        full_name: &str,
        private: bool,
    ) -> Result<(), ClientError> {
        let response = self
            .send_with_retry(|| {

                self.inner
                    .patch(format!("{}/repos/{full_name}", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({ "private": private }))
            })
            .await?;
        check_success(response)?;
        Ok(())
    }

    /// 保管単位を削除。
    pub async fn delete_repo(&self, token: &Token, full_name: &str) -> Result<(), ClientError> {
        let response = self
            .send_with_retry(|| {

                self.inner
                    .delete(format!("{}/repos/{full_name}", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
            })
            .await?;
        check_success(response)?;
        Ok(())
    }

    /// ルート直下のコンテンツ一覧を取得（ツリー走査の最小単位）。
    pub async fn list_contents(
        &self,
        token: &Token,
        full_name: &str,
        path: Option<&str>,
    ) -> Result<Vec<ContentEntry>, ClientError> {
        let url = match path {
            Some(p) if !p.is_empty() => {
                format!("{}/repos/{full_name}/contents/{p}", self.base_url)
            }
            _ => format!("{}/repos/{full_name}/contents", self.base_url),
        };
        let response = self
            .send_with_retry(|| {

                self.inner
                    .get(url.clone())
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<Vec<ContentEntry>>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// 単一ファイルを取得。
    pub async fn get_file(
        &self,
        token: &Token,
        full_name: &str,
        path: &str,
    ) -> Result<FileEntry, ClientError> {
        let response = self
            .send_with_retry(|| {

                self.inner
                    .get(format!("{}/repos/{full_name}/contents/{path}", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<FileEntry>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// atomic コミット（Git Data API: blob → tree → commit → ref 更新）。
    ///
    /// ref 先端の衝突時は再取得して 1 回だけリトライする（設計 §8 に準拠）。
    pub async fn commit_files(
        &self,
        token: &Token,
        full_name: &str,
        branch: &str,
        files: &[NewFile],
    ) -> Result<String, ClientError> {
        let mut last_error = ClientError::Conflict;
        for _ in 0..2 {
            match self.try_commit(token, full_name, branch, files).await {
                Ok(sha) => return Ok(sha),
                Err(ClientError::Conflict) => last_error = ClientError::Conflict,
                Err(e) => return Err(e),
            }
        }
        Err(last_error)
    }

    async fn try_commit(
        &self,
        token: &Token,
        full_name: &str,
        branch: &str,
        files: &[NewFile],
    ) -> Result<String, ClientError> {
        let headers = self.headers(Some(token.as_str()));
        let head = self
            .get_ref_sha(token, full_name, branch)
            .await?
            .ok_or(ClientError::NotFound)?;

        // 1. blob 作成
        let mut tree: serde_json::Value = serde_json::json!([]);
        for file in files {
            let encoded = match file.encoding {
                crate::ports::FileEncoding::Text => {
                    use base64::engine::general_purpose::STANDARD;
                    use base64::Engine;
                    STANDARD.encode(file.content.as_bytes())
                }
                crate::ports::FileEncoding::Base64 => file.content.clone(),
            };
            let blob_response = self
                .send_with_retry(|| {

                    self.inner
                        .post(format!("{}/repos/{full_name}/git/blobs", self.base_url))
                        .headers(self.headers(Some(token.as_str())))
                        .timeout(self.timeout_write)
                        .json(&serde_json::json!({
                            "content": encoded,
                            "encoding": "base64",
                        }))
            })
                .await?;
            let blob_response = check_success(blob_response)?;
            let blob: serde_json::Value = blob_response
                .json()
                .await
                .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))?;
            let blob_sha = blob["sha"]
                .as_str()
                .ok_or_else(|| ClientError::InvalidResponse("blob の sha が無い".into()))?;
            tree.as_array_mut()
                .unwrap()
                .push(serde_json::json!({
                    "path": file.path,
                    "mode": "100644",
                    "type": "blob",
                    "sha": blob_sha,
                }));
        }

        // 2. tree 作成
        let tree_response = self
            .send_with_retry(|| {

                self.inner
                    .post(format!("{}/repos/{full_name}/git/trees", self.base_url))
                    .headers(headers.clone())
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({ "base_tree": head, "tree": tree }))
            })
            .await?;
        let tree_response = check_success(tree_response)?;
        let tree_payload: serde_json::Value = tree_response
            .json()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))?;
        let tree_sha = tree_payload["sha"]
            .as_str()
            .ok_or_else(|| ClientError::InvalidResponse("tree の sha が無い".into()))?;

        // 3. commit 作成
        let message = format!("novnow-hub commit [idem:{}]", sha256_short(full_name, branch, files));
        let commit_response = self
            .send_with_retry(|| {

                self.inner
                    .post(format!("{}/repos/{full_name}/git/commits", self.base_url))
                    .headers(headers.clone())
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({
                        "message": message,
                        "tree": tree_sha,
                        "parents": [head],
                    }))
            })
            .await?;
        let commit_response = check_success(commit_response)?;
        let commit_payload: serde_json::Value = commit_response
            .json()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))?;
        let commit_sha = commit_payload["sha"]
            .as_str()
            .ok_or_else(|| ClientError::InvalidResponse("commit の sha が無い".into()))?;

        // 4. ref 更新
        let ref_response = self
            .send_with_retry(|| {

                self.inner
                    .patch(format!(
                        "{}/repos/{full_name}/git/refs/heads/{branch}",
                        self.base_url
                    ))
                    .headers(headers.clone())
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({ "sha": commit_sha }))
            })
            .await?;
        check_success(ref_response)?;
        Ok(commit_sha.to_string())
    }

    async fn get_ref_sha(
        &self,
        token: &Token,
        full_name: &str,
        branch: &str,
    ) -> Result<Option<String>, ClientError> {
        let response = self
            .send_with_retry(|| {

                self.inner
                    .get(format!(
                        "{}/repos/{full_name}/git/ref/heads/{branch}",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))?;
        Ok(payload["object"]["sha"].as_str().map(|s| s.to_string()))
    }
}

fn check_success(response: reqwest::Response) -> Result<reqwest::Response, ClientError> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(GitHubClient::map_status(response.status()))
    }
}

/// 冪等コミットメッセージ用の短いハッシュ。
fn sha256_short(full_name: &str, branch: &str, files: &[NewFile]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(full_name.as_bytes());
    hasher.update(branch.as_bytes());
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update(file.content.as_bytes());
    }
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).take(8).collect()
}

/// OAuth クライアント設定（環境変数由来。値自体はここに書かない）。
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub redirect_uri: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub id: i64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: String,
}

#[derive(Debug, Deserialize)]
pub struct RepoMeta {
    pub full_name: String,
    pub default_branch: String,
    pub private: bool,
    pub etag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContentEntry {
    pub path: String,
    pub sha: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Debug, Deserialize)]
pub struct FileEntry {
    pub content: String,
    pub encoding: FileEncoding,
    pub sha: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileEncoding {
    Text,
    Base64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_deterministic_and_base64url() {
        let a = GitHubClient::pkce_challenge("verifier-123");
        let b = GitHubClient::pkce_challenge("verifier-123");
        assert_eq!(a, b);
        // S256 の 32 バイト → base64url は 43 文字。
        assert_eq!(a.len(), 43);
        // base64url 文字セットに収まる。
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn pkce_challenge_differs_per_verifier() {
        let a = GitHubClient::pkce_challenge("verifier-1");
        let b = GitHubClient::pkce_challenge("verifier-2");
        assert_ne!(a, b);
    }

    #[test]
    fn map_status_maps_known_codes() {
        use reqwest::StatusCode;
        assert!(matches!(
            GitHubClient::map_status(StatusCode::NOT_FOUND),
            ClientError::NotFound
        ));
        assert!(matches!(
            GitHubClient::map_status(StatusCode::CONFLICT),
            ClientError::Conflict
        ));
        assert!(matches!(
            GitHubClient::map_status(StatusCode::FORBIDDEN),
            ClientError::PermissionDenied
        ));
        assert!(matches!(
            GitHubClient::map_status(StatusCode::TOO_MANY_REQUESTS),
            ClientError::RateLimited
        ));
        assert!(matches!(
            GitHubClient::map_status(StatusCode::BAD_GATEWAY),
            ClientError::Unavailable(_)
        ));
    }
}
