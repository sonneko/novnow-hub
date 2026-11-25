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
#[derive(Clone)]
pub struct GitHubClient {
    inner: reqwest::Client,
    /// API ベース URL（`https://api.github.com`）。REST API 用。
    base_url: String,
    /// 認証ベース URL（`https://github.com`）。OAuth のトークン交換等に用いる。
    auth_base: String,
    timeout_read: Duration,
    timeout_write: Duration,
}

impl GitHubClient {
    pub fn new(
        base_url: &str,
        auth_base: &str,
        timeout_read: Duration,
        timeout_write: Duration,
    ) -> Self {
        let inner = reqwest::Client::builder()
            .timeout(timeout_write)
            .build()
            .expect("reqwest クライアントの初期化に失敗");
        Self {
            inner,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_base: auth_base.trim_end_matches('/').to_string(),
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
            502..=504 => ClientError::Unavailable("上流障害".into()),
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

    /// トークン交換エンドポイントの URL。
    ///
    /// 認証ベース URL（`github.com`）配下の `/login/oauth/access_token`。
    /// API ベース URL（`api.github.com`）ではない。
    fn exchange_code_url(auth_base: &str) -> String {
        format!("{}/login/oauth/access_token", auth_base.trim_end_matches('/'))
    }

    /// トークン交換のリクエストボディ。
    ///
    /// `client_secret` と `redirect_uri` を含む（GitHub の交換仕様）。
    fn exchange_code_body(code: &str, code_verifier: &str, oauth: &OAuthConfig) -> serde_json::Value {
        serde_json::json!({
            "client_id": oauth.client_id,
            "client_secret": oauth.client_secret,
            "code": code,
            "code_verifier": code_verifier,
            "redirect_uri": oauth.redirect_uri,
        })
    }

    /// 認可コードをアクセストークンへ交換（PKCE）。
    ///
    /// トークン交換エンドポイントは認証ベース URL（`auth_base`）配下の
    /// `/login/oauth/access_token`。ボディには PKCE に加え OAuth App の
    /// `client_secret` と `redirect_uri` を含む（GitHub の交換仕様）。
    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
        oauth: &OAuthConfig,
    ) -> Result<Token, ClientError> {
        let body = Self::exchange_code_body(code, code_verifier, oauth);
        let auth_base = self.auth_base.clone();
        let response = self
            .send_with_retry(|| {
                self.inner
                    .post(Self::exchange_code_url(&auth_base))
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
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_matches('"').to_string());
        let mut repo: RepoMeta = response
            .json::<RepoMeta>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))?;
        repo.etag = etag;
        Ok(repo)
    }

    /// 保管単位のメタを取得。
    ///
    /// `etag` は GitHub の repo JSON には含まれない（ETag はレスポンス
    /// ヘッダ）。ボディからは取得できないため、ヘッダの `ETag` から読む。
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
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_matches('"').to_string());
        let mut repo: RepoMeta = response
            .json::<RepoMeta>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))?;
        repo.etag = etag;
        Ok(repo)
    }

    /// Fork（派生リポジトリの作成）を要求する。
    ///
    /// GitHub は Fork を非同期で作成するため、`POST /repos/{owner}/{repo}/forks`
    /// （202 受理）の後、認証ユーザーの fork リポジトリを `GET /repos/{user}/{repo}`
    /// でポーリングして完成を待つ。上限回数まで待てば最後の結果を返す。
    pub async fn create_fork(
        &self,
        token: &Token,
        full_name: &str,
    ) -> Result<RepoMeta, ClientError> {
        // Fork 要求（202 受理）。
        let response = self
            .send_with_retry(|| {

                self.inner
                    .post(format!("{}/repos/{full_name}/forks", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
            })
            .await?;
        check_success(response)?;

        // 認証ユーザーのログイン名を取得し、fork リポジトリをポーリングする。
        let user = self.get_user(token).await?;
        let fork_full_name = format!("{}/{}", user.login, full_name.rsplit('/').next().unwrap_or(full_name));

        let mut last_error = ClientError::Unavailable("fork の完了を待てませんでした".into());
        for _ in 0..10 {
            match self.get_repo(token, &fork_full_name).await {
                Ok(repo) => return Ok(repo),
                Err(e) => last_error = e,
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        Err(last_error)
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

    // ---- M4: Pull Requests（変更提案） ----

    /// Pull Request を作成。
    pub async fn create_pull(
        &self,
        token: &Token,
        full_name: &str,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
    ) -> Result<PrJson, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .post(format!("{}/repos/{full_name}/pulls", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({
                        "title": title,
                        "body": body,
                        "head": head,
                        "base": base,
                    }))
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<PrJson>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Pull Request を取得。
    pub async fn get_pull(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
    ) -> Result<PrJson, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .get(format!("{}/repos/{full_name}/pulls/{number}", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<PrJson>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Pull Request の一覧（state フィルタ）。
    pub async fn list_pulls(
        &self,
        token: &Token,
        full_name: &str,
        state: Option<&str>,
    ) -> Result<Vec<PrJson>, ClientError> {
        let url = match state {
            Some(s) if !s.is_empty() => {
                format!("{}/repos/{full_name}/pulls?state={s}", self.base_url)
            }
            _ => format!("{}/repos/{full_name}/pulls", self.base_url),
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
            .json::<Vec<PrJson>>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Pull Request の差分ファイル一覧。
    pub async fn list_pull_files(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
    ) -> Result<Vec<PrFile>, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .get(format!(
                        "{}/repos/{full_name}/pulls/{number}/files",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<Vec<PrFile>>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Pull Request のレビュー一覧（承認数の算出用）。
    pub async fn list_pull_reviews(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
    ) -> Result<Vec<ReviewJson>, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .get(format!(
                        "{}/repos/{full_name}/pulls/{number}/reviews",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<Vec<ReviewJson>>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Pull Request へレビューを記録。
    pub async fn create_pull_review(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
        event: &str,
        body: Option<&str>,
    ) -> Result<(), ClientError> {
        let mut body_json = serde_json::json!({ "event": event });
        if let Some(b) = body {
            body_json["body"] = serde_json::json!(b);
        }
        let response = self
            .send_with_retry(|| {
                self.inner
                    .post(format!(
                        "{}/repos/{full_name}/pulls/{number}/reviews",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(&body_json)
            })
            .await?;
        check_success(response)?;
        Ok(())
    }

    /// Pull Request をマージ。
    pub async fn merge_pull(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
    ) -> Result<MergeJson, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .post(format!(
                        "{}/repos/{full_name}/pulls/{number}/merge",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({ "merge_method": "merge" }))
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<MergeJson>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Pull Request の状態を更新（クローズ等）。
    pub async fn update_pull(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
        state: &str,
    ) -> Result<PrJson, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .patch(format!("{}/repos/{full_name}/pulls/{number}", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({ "state": state }))
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<PrJson>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Pull Request の会話コメント一覧（PR は Issue として扱う）。
    pub async fn list_issue_comments(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
    ) -> Result<Vec<CommentJson>, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .get(format!(
                        "{}/repos/{full_name}/issues/{number}/comments",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<Vec<CommentJson>>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    // ---- M4: Issues（課題） ----

    /// Issue を作成。
    pub async fn create_issue(
        &self,
        token: &Token,
        full_name: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<IssueJson, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .post(format!("{}/repos/{full_name}/issues", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({
                        "title": title,
                        "body": body,
                        "labels": labels,
                    }))
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<IssueJson>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Issue を取得。
    pub async fn get_issue(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
    ) -> Result<IssueJson, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .get(format!("{}/repos/{full_name}/issues/{number}", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<IssueJson>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Issue の一覧（state・label フィルタ）。
    pub async fn list_issues(
        &self,
        token: &Token,
        full_name: &str,
        state: Option<&str>,
        label: Option<&str>,
    ) -> Result<Vec<IssueJson>, ClientError> {
        let mut url = format!("{}/repos/{full_name}/issues", self.base_url);
        let mut params: Vec<String> = Vec::new();
        if let Some(s) = state {
            if !s.is_empty() {
                params.push(format!("state={s}"));
            }
        }
        if let Some(l) = label {
            if !l.is_empty() {
                params.push(format!("labels={l}"));
            }
        }
        if !params.is_empty() {
            url.push_str(&format!("?{}", params.join("&")));
        }
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
            .json::<Vec<IssueJson>>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Issue を更新。
    pub async fn update_issue(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
        body: &serde_json::Value,
    ) -> Result<IssueJson, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .patch(format!("{}/repos/{full_name}/issues/{number}", self.base_url))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(body)
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<IssueJson>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    /// Issue へコメント。
    pub async fn create_issue_comment(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
        body: &str,
    ) -> Result<CommentJson, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .post(format!(
                        "{}/repos/{full_name}/issues/{number}/comments",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({ "body": body }))
            })
            .await?;
        let response = check_success(response)?;
        response
            .json::<CommentJson>()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))
    }

    // ---- M4: Check Runs（チェック状態） ----

    /// コミットのチェックラン一覧。
    pub async fn list_check_runs(
        &self,
        token: &Token,
        full_name: &str,
        ref_name: &str,
    ) -> Result<Vec<CheckRunJson>, ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .get(format!(
                        "{}/repos/{full_name}/commits/{ref_name}/check-runs",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_read)
            })
            .await?;
        let response = check_success(response)?;
        let payload: CheckRunsResponse = response
            .json()
            .await
            .map_err(|e| ClientError::InvalidResponse(format!("ボディ解析失敗: {e}")))?;
        Ok(payload.check_runs)
    }

    // ---- M4: Auto-merge（自動マージ） ----

    /// 自動マージを予約。
    pub async fn set_automerge(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
        required_approvals: u64,
        required_checks: &[String],
        // GitHub の自動マージは競合のある PR をマージしないため、`require_no_conflicts`
        // は GitHub 側の仕組みによって満たされる。明示的なフラグは存在しない。
        _require_no_conflicts: bool,
    ) -> Result<(), ClientError> {
        let required_status_checks = if required_checks.is_empty() {
            serde_json::json!(null)
        } else {
            serde_json::json!({
                "strict": true,
                "contexts": required_checks,
            })
        };
        let response = self
            .send_with_retry(|| {
                self.inner
                    .put(format!(
                        "{}/repos/{full_name}/pulls/{number}/merge",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
                    .json(&serde_json::json!({
                        "merge_method": "merge",
                        "required_status_checks": required_status_checks,
                        "required_approvals": required_approvals,
                        "required_pull_request_reviews": {
                            "dismissal_restrictions": null,
                            "dismiss_stale_reviews": false,
                            "require_code_owner_reviews": false,
                            "required_approving_review_count": required_approvals,
                        },
                        "allow_update_by_squash": true,
                        "allow_update_by_rebase": true,
                    }))
            })
            .await?;
        check_success(response)?;
        Ok(())
    }

    /// 自動マージを取消。
    pub async fn delete_automerge(
        &self,
        token: &Token,
        full_name: &str,
        number: u64,
    ) -> Result<(), ClientError> {
        let response = self
            .send_with_retry(|| {
                self.inner
                    .delete(format!(
                        "{}/repos/{full_name}/pulls/{number}/merge",
                        self.base_url
                    ))
                    .headers(self.headers(Some(token.as_str())))
                    .timeout(self.timeout_write)
            })
            .await?;
        // 204 成功。既に予約が無い場合も GitHub は成功を返す。
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(Self::map_status(status))
        }
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
    pub client_secret: String,
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
    /// GitHub の repo JSON には含まれない。レスポンスヘッダの `ETag` から
    /// `get_repo` が設定する。ヘッダが無ければ `None`。
    pub etag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContentEntry {
    pub path: String,
    pub sha: String,
    /// GitHub の contents API は `type`（"file"/"dir"/"symlink"/"submodule"）を返す。
    /// `is_dir` は返さないため、種別はこちらで判定する。
    #[serde(rename = "type")]
    pub entry_type: String,
    /// ディレクトリでは欠損または 0 の場合がある。
    #[serde(default)]
    pub size: Option<u64>,
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

// ---- M4: GitHub の Pull Request／Issue／Check Run 応答型 ----

/// Pull Request のユーザー。
#[derive(Debug, Deserialize)]
pub struct PrUser {
    pub login: String,
}

/// Pull Request の ref（base／head）。
#[derive(Debug, Deserialize)]
pub struct PrRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

/// Pull Request の応答（M4 で必要な項目のみ）。
#[derive(Debug, Deserialize)]
pub struct PrJson {
    pub number: i64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub state: String,
    #[serde(default)]
    pub user: Option<PrUser>,
    pub base: PrRef,
    pub head: PrRef,
    #[serde(default)]
    pub merged: bool,
    #[serde(default)]
    pub mergeable: Option<bool>,
    #[serde(default)]
    pub mergeable_state: Option<String>,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub changed_files: u64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Pull Request の差分ファイル。
#[derive(Debug, Deserialize)]
pub struct PrFile {
    pub filename: String,
    pub status: String,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub patch: Option<String>,
}

/// レビューの応答（承認数の算出用）。
#[derive(Debug, Deserialize)]
pub struct ReviewJson {
    pub state: String,
}

/// マージ結果の応答。
#[derive(Debug, Deserialize)]
pub struct MergeJson {
    pub merged: bool,
    pub sha: String,
    #[serde(default)]
    pub merged_at: Option<String>,
}

/// コメントの応答。
#[derive(Debug, Deserialize)]
pub struct CommentJson {
    pub id: i64,
    #[serde(default)]
    pub user: Option<PrUser>,
    pub body: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub line: Option<i64>,
    pub created_at: String,
}

/// ラベル。
#[derive(Debug, Deserialize)]
pub struct LabelJson {
    pub name: String,
}

/// Issue の応答（M4 で必要な項目のみ）。
#[derive(Debug, Deserialize)]
pub struct IssueJson {
    pub number: i64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<LabelJson>,
    #[serde(default)]
    pub user: Option<PrUser>,
    #[serde(default)]
    pub assignee: Option<PrUser>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Issues API の一覧は Pull Request も返す。PR は `pull_request` キーで識別し
    /// 除外する。
    #[serde(default)]
    pub pull_request: Option<serde_json::Value>,
}

/// チェックラン一覧の応答。
#[derive(Debug, Deserialize)]
pub struct CheckRunsResponse {
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub check_runs: Vec<CheckRunJson>,
}

/// チェックラン。
#[derive(Debug, Deserialize)]
pub struct CheckRunJson {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
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

    #[test]
    fn exchange_code_url_targets_auth_host() {
        // 修正1: 交換先は api.github.com ではなく github.com 配下。
        let url = GitHubClient::exchange_code_url("https://github.com");
        assert_eq!(url, "https://github.com/login/oauth/access_token");
        assert!(!url.contains("api.github.com"));
        // 末尾スラッシュを除去。
        let url2 = GitHubClient::exchange_code_url("https://github.com/");
        assert_eq!(url2, "https://github.com/login/oauth/access_token");
    }

    #[test]
    fn exchange_code_body_includes_secret_and_redirect() {
        // 修正1: ボディには client_secret と redirect_uri を含む。
        let oauth = OAuthConfig {
            client_id: "cid".into(),
            client_secret: "secret-value".into(),
            redirect_uri: "https://hub.example/callback".into(),
        };
        let body = GitHubClient::exchange_code_body("the-code", "the-verifier", &oauth);
        assert_eq!(body["client_id"], "cid");
        assert_eq!(body["client_secret"], "secret-value");
        assert_eq!(body["code"], "the-code");
        assert_eq!(body["code_verifier"], "the-verifier");
        assert_eq!(body["redirect_uri"], "https://hub.example/callback");
    }

    #[test]
    fn content_entry_deserializes_github_file_json() {
        // 修正2: GitHub は is_dir を返さず type を返す。file 型を読む。
        let json = r#"{
            "path": "README.md",
            "sha": "abc123",
            "size": 1234,
            "type": "file"
        }"#;
        let entry: ContentEntry = serde_json::from_str(json).expect("file 型 JSON を読める");
        assert_eq!(entry.path, "README.md");
        assert_eq!(entry.entry_type, "file");
        assert_eq!(entry.size, Some(1234));
    }

    #[test]
    fn content_entry_deserializes_github_dir_json() {
        // 修正2: ディレクトリは size 無し／0 の場合がある。
        let json = r#"{
            "path": "docs",
            "sha": "def456",
            "type": "dir"
        }"#;
        let entry: ContentEntry = serde_json::from_str(json).expect("dir 型 JSON を読める");
        assert_eq!(entry.entry_type, "dir");
        assert_eq!(entry.size, None);
    }

    #[test]
    fn content_entry_deserializes_symlink_json() {
        // 修正2: symlink 等の別種別も type から読む。
        let json = r#"{
            "path": "link",
            "sha": "ghi789",
            "size": 0,
            "type": "symlink"
        }"#;
        let entry: ContentEntry = serde_json::from_str(json).expect("symlink 型 JSON を読める");
        assert_eq!(entry.entry_type, "symlink");
        assert_eq!(entry.size, Some(0));
    }

    #[test]
    fn repo_meta_etag_optional_in_body() {
        // 修正7: repo JSON に etag は無い（ヘッダ由来）。ボディ解析は通る。
        let json = r#"{
            "full_name": "owner/repo",
            "default_branch": "main",
            "private": false
        }"#;
        let meta: RepoMeta = serde_json::from_str(json).expect("etag 無しでも読める");
        assert_eq!(meta.full_name, "owner/repo");
        assert!(meta.etag.is_none());
    }
}
