//! 統合テスト用の補助（multipart 本体生成・zip 生成・リクエスト送出）。
//!
//! `POST /projects` の公開フローを router 経由で検証するためのヘルパー。
//! GitHub の保管単位作成は本番トークンが必要なため、検証は認証ゲートと
//! multipart 受領→novnow 検証の失敗時挙動（中間状態を残さない）に切り分ける。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;

use crate::app_state::AppState;
use crate::config::Config;

/// テスト用の Config を組み立てる。
pub fn test_config() -> Config {
    Config {
        contract_version: "1.0.0".into(),
        minimum_client_version: "1.0.0".into(),
        github_oauth_client_id: None,
        github_oauth_client_secret: None,
        github_oauth_redirect_uri: None,
        github_api_base: "https://api.github.com".into(),
        github_auth_base: "https://github.com".into(),
        access_token_ttl_secs: 3600,
        refresh_token_ttl_secs: 30 * 24 * 3600,
        bind_addr: "127.0.0.1".into(),
        read_timeout_secs: 5,
        write_timeout_secs: 30,
        commit_timeout_secs: 120,
        port: 8080,
        hub_public_base: "http://localhost:4321".into(),
        webhook_secret: Some("test-webhook-secret".into()),
    }
}

/// テスト用の AppState を組み立てる。
pub fn test_state() -> AppState {
    crate::build_app_state(test_config())
}

/// 指定した GitHub API ベース URL を向いたテスト用 Config を組み立てる。
pub fn test_config_with_github_base(github_api_base: &str) -> Config {
    let mut config = test_config();
    config.github_api_base = github_api_base.to_string();
    config
}

/// 指定した GitHub API ベース URL を向いたテスト用 AppState を組み立てる。
pub fn test_state_with_github_base(github_api_base: &str) -> AppState {
    crate::build_app_state(test_config_with_github_base(github_api_base))
}

/// 統合テスト用にプロジェクトを登録簿へ登録する（projectId → shell の対応）。
pub fn register_test_project(state: &AppState, project_id: &str, shell_id: &str) {
    use crate::ports::ProjectVisibility;
    use crate::registry::{ProjectRecord, DEFAULT_BRANCH};
    state.projects.register(ProjectRecord {
        project_id: project_id.to_string(),
        title: "Test Project".to_string(),
        description: None,
        tags: Vec::new(),
        owner_handle: "octo".to_string(),
        license: "cc-by".to_string(),
        derivative_policy: "allowed".to_string(),
        visibility: ProjectVisibility::Public,
        shell_id: shell_id.to_string(),
        default_branch: DEFAULT_BRANCH.to_string(),
        etag: "etag".to_string(),
        created_at: 0,
        updated_at: 0,
    });
}

/// multipart/form-data 本体を構築する。
///
/// `parts` は (フィールド名, ファイル名の有無, 内容) の列。
pub fn build_multipart(
    boundary: &str,
    parts: &[(&str, Option<&str>, &[u8])],
) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, filename, content) in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        if let Some(fname) = filename {
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"{fname}\"\r\n"
                )
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

/// multipart リクエストを構築する。
pub fn multipart_request(path: &str, body: &[u8], token: Option<&str>) -> Request<Body> {
    let boundary = "----testboundary7MA4YWxkTrZu0gW";
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .header("content-length", body.len().to_string());
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_vec())).unwrap()
}

/// GET リクエストを構築する（パスにクエリ文字列を含む）。
pub fn get_request(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

/// レスポンスボディを文字列として取得する。
pub async fn response_text(response: axum::response::Response) -> (StatusCode, String) {
    let status = response.status();
    let collected = response.into_body().collect().await.unwrap();
    let text = String::from_utf8_lossy(&collected.to_bytes()).into_owned();
    (status, text)
}

/// テスト用 router を返す。
pub fn test_router(state: AppState) -> Router {
    crate::build_router(state)
}

// ============================================================================
// M4 コラボレーションルートの HTTP 統合テスト用 mock GitHub API サーバー。
// ルート→ポート→GitHub 呼び出しの写像を、ネットワーク非依存で検証する。
// ============================================================================

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

/// mock が記録した GitHub への発信リクエスト。
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
}

/// mock GitHub サーバーの共有状態。
#[derive(Clone, Default)]
pub struct MockGitHub {
    pub recorded: Arc<Mutex<Vec<RecordedRequest>>>,
    pub prs: Arc<Mutex<Vec<serde_json::Value>>>,
    pub reviews: Arc<Mutex<HashMap<i64, Vec<serde_json::Value>>>>,
    /// contents API が返すファイル一覧（パス → 内容）。
    pub files: Arc<Mutex<HashMap<String, String>>>,
}

impl MockGitHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// PR の固定フィクサを設定する。
    pub fn set_prs(&self, prs: Vec<serde_json::Value>) {
        *self.prs.lock().unwrap() = prs;
    }

    /// PR 番号ごとのレビューを設定する。
    pub fn set_reviews(&self, reviews: HashMap<i64, Vec<serde_json::Value>>) {
        *self.reviews.lock().unwrap() = reviews;
    }

    /// contents API が返すファイル一覧を設定する（パス → 内容）。
    pub fn set_files(&self, files: HashMap<String, String>) {
        *self.files.lock().unwrap() = files;
    }

    /// 記録されたリクエストを返す。
    pub fn recorded(&self) -> Vec<RecordedRequest> {
        self.recorded.lock().unwrap().clone()
    }

    /// 指定メソッド＋パス（クエリ込み）に部分一致するリクエストが存在するか。
    pub fn has(&self, method: &str, path_fragment: &str) -> bool {
        self.recorded
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.method == method && r.path.contains(path_fragment))
    }

    /// mock サーバーを起動し、ベース URL を返す。
    pub async fn start(&self) -> String {
        let app = Router::new()
            .route("/repos/{owner}/{repo}", get(repo))
            .route(
                "/repos/{owner}/{repo}/pulls",
                get(list_pulls).post(create_pull),
            )
            .route(
                "/repos/{owner}/{repo}/pulls/{n}",
                get(get_pull).patch(update_pull),
            )
            .route("/repos/{owner}/{repo}/pulls/{n}/files", get(pull_files))
            .route(
                "/repos/{owner}/{repo}/pulls/{n}/reviews",
                get(pull_reviews).post(post_review),
            )
            .route(
                "/repos/{owner}/{repo}/pulls/{n}/merge",
                axum::routing::put(put_merge).delete(delete_merge).post(post_merge),
            )
            .route(
                "/repos/{owner}/{repo}/issues",
                get(list_issues).post(create_issue),
            )
            .route(
                "/repos/{owner}/{repo}/issues/{n}",
                get(get_issue).patch(update_issue),
            )
            .route(
                "/repos/{owner}/{repo}/issues/{n}/comments",
                get(list_comments).post(post_comment),
            )
            .route(
                "/repos/{owner}/{repo}/commits/{ref}/check-runs",
                get(check_runs),
            )
            .route(
                "/repos/{owner}/{repo}/contents",
                get(list_contents_root),
            )
            .route(
                "/repos/{owner}/{repo}/contents/{*path}",
                get(get_contents),
            )
            .with_state(self.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }
}

fn json_ok(value: serde_json::Value) -> Response {
    let mut resp = (StatusCode::OK, axum::Json(value)).into_response();
    resp.headers_mut()
        .insert("ETag", "\"repo-etag\"".parse().unwrap());
    resp
}

fn pr_json(number: i64, pr_state: &str, merged: bool) -> serde_json::Value {
    serde_json::json!({
        "number": number,
        "title": format!("T{number}"),
        "body": "B",
        "state": pr_state,
        "user": {"login": "alice"},
        "base": {"ref": "main"},
        "head": {"ref": "feat"},
        "merged": merged,
        "additions": 5,
        "deletions": 2,
        "changed_files": 1,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-02T00:00:00Z"
    })
}

fn issue_json(number: i64) -> serde_json::Value {
    serde_json::json!({
        "number": number,
        "title": format!("I{number}"),
        "body": "x",
        "state": "open",
        "labels": [{"name": "bug"}],
        "user": {"login": "alice"},
        "assignee": null,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
    })
}

async fn read_req(req: Request<Body>) -> (String, String) {
    let method = req.method().to_string();
    // パス＋クエリを記録する（state 等の絞り込みが発信されることの検証用）。
    let uri = req.uri().to_string();
    // ボディは読み飛ばす（記録はメソッド＋パス＋クエリのみ）。
    let _ = req.into_body().collect().await;
    (method, uri)
}

async fn repo(State(_mock): State<MockGitHub>) -> Response {
    json_ok(serde_json::json!({
        "full_name": "octo/demo",
        "default_branch": "main",
        "private": false
    }))
}

async fn list_pulls(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    let prs = mock.prs.lock().unwrap().clone();
    json_ok(serde_json::json!(prs))
}

async fn create_pull(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    json_ok(pr_json(7, "open", false))
}

async fn get_pull(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    let n: i64 = path.rsplit('/').next().unwrap_or("0").parse().unwrap_or(0);
    let prs = mock.prs.lock().unwrap().clone();
    let pr = prs
        .into_iter()
        .find(|p| p["number"].as_i64() == Some(n))
        .unwrap_or_else(|| pr_json(n, "open", false));
    json_ok(pr)
}

async fn update_pull(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    let n: i64 = path.rsplit('/').next().unwrap_or("0").parse().unwrap_or(0);
    json_ok(pr_json(n, "closed", false))
}

async fn pull_files(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    json_ok(serde_json::json!([{
        "filename": "a.md",
        "status": "modified",
        "additions": 3,
        "deletions": 1,
        "patch": "@@ -1 +1 @@\n-old\n+new"
    }]))
}

async fn pull_reviews(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    let n: i64 = path.rsplit('/').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let reviews = mock.reviews.lock().unwrap().clone();
    let list = reviews
        .get(&n)
        .cloned()
        .unwrap_or_else(|| vec![serde_json::json!({"state": "APPROVED"})]);
    json_ok(serde_json::json!(list))
}

async fn post_review(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    StatusCode::CREATED.into_response()
}

async fn post_merge(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    json_ok(serde_json::json!({
        "merged": true,
        "sha": "abc123",
        "merged_at": "2026-01-03T00:00:00Z"
    }))
}

async fn put_merge(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    StatusCode::NO_CONTENT.into_response()
}

async fn delete_merge(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    StatusCode::NO_CONTENT.into_response()
}

async fn create_issue(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    json_ok(issue_json(3))
}

async fn list_issues(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    json_ok(serde_json::json!([issue_json(3)]))
}

async fn get_issue(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    let n: i64 = path.rsplit('/').next().unwrap_or("0").parse().unwrap_or(0);
    json_ok(issue_json(n))
}

async fn update_issue(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    let n: i64 = path.rsplit('/').next().unwrap_or("0").parse().unwrap_or(0);
    json_ok(issue_json(n))
}

async fn list_comments(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    json_ok(serde_json::json!([{
        "id": 1,
        "user": {"login": "bob"},
        "body": "hi",
        "path": null,
        "line": null,
        "created_at": "2026-01-01T00:00:00Z"
    }]))
}

async fn post_comment(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    json_ok(serde_json::json!({
        "id": 2,
        "user": {"login": "bob"},
        "body": "msg",
        "path": null,
        "line": null,
        "created_at": "2026-01-01T00:00:00Z"
    }))
}

async fn check_runs(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    json_ok(serde_json::json!({
        "total_count": 1,
        "check_runs": [{
            "name": "novnow-verify",
            "status": "completed",
            "conclusion": "success",
            "html_url": "https://github.com/octo/demo/runs/1"
        }]
    }))
}

/// contents API のルート直下の一覧を返す（ディレクトリ entry を含む）。
async fn list_contents_root(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    let files = mock.files.lock().unwrap().clone();
    let mut entries = Vec::new();
    // ルート直下のディレクトリとファイルを返す。
    let mut dirs: Vec<String> = files
        .keys()
        .filter_map(|p| p.split('/').next().map(|s| s.to_string()))
        .collect();
    dirs.sort();
    dirs.dedup();
    for d in &dirs {
        if files.keys().any(|p| p.starts_with(&format!("{d}/"))) {
            entries.push(serde_json::json!({
                "path": d, "sha": format!("sha-{d}"), "type": "dir"
            }));
        }
    }
    for p in files.keys() {
        if !p.contains('/') {
            entries.push(serde_json::json!({
                "path": p, "sha": format!("sha-{p}"), "type": "file", "size": 10
            }));
        }
    }
    json_ok(serde_json::json!(entries))
}

/// contents API のパス指定の一覧／ファイルを返す。
async fn get_contents(State(mock): State<MockGitHub>, req: Request<Body>) -> Response {
    let (method, path) = read_req(req).await;
    mock.recorded.lock().unwrap().push(RecordedRequest { method, path: path.clone() });
    // パスは `/repos/{owner}/{repo}/contents/{*path}`。{*path} を取り出す。
    let content_path = path
        .split("/contents/")
        .nth(1)
        .unwrap_or("")
        .trim_matches('/')
        .to_string();
    let files = mock.files.lock().unwrap().clone();
    if let Some(content) = files.get(&content_path) {
        // ファイル: base64 で返す（client が Text/Base64 を判定）。
        let encoded = base64_encode(content.as_bytes());
        let body = serde_json::json!({
            "content": encoded,
            "encoding": "base64",
            "sha": format!("sha-{content_path}")
        });
        return json_ok(body);
    }
    // ディレクトリ: 配下のファイルを集約して entry 一覧を返す。
    let prefix = format!("{content_path}/");
    let mut entries = Vec::new();
    // 真のサブディレクトリのみを dir entry とする（{prefix}{d}/ で始まるパスが存在する）。
    let mut subdirs: Vec<String> = files
        .keys()
        .filter_map(|p| p.strip_prefix(&prefix).and_then(|rest| rest.split('/').next()))
        .map(|s| s.to_string())
        .filter(|d| !d.is_empty() && files.keys().any(|p| p.starts_with(&format!("{prefix}{d}/"))))
        .collect();
    subdirs.sort();
    subdirs.dedup();
    for d in &subdirs {
        entries.push(serde_json::json!({
            "path": format!("{prefix}{d}"), "sha": format!("sha-{d}"), "type": "dir"
        }));
    }
    for p in files.keys() {
        if let Some(rest) = p.strip_prefix(&prefix) {
            if !rest.contains('/') {
                entries.push(serde_json::json!({
                    "path": p, "sha": format!("sha-{p}"), "type": "file", "size": 10
                }));
            }
        }
    }
    json_ok(serde_json::json!(entries))
}

/// 最小限の base64 エンコード（テスト用）。
fn base64_encode(data: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.encode(data)
}

// ============================================================================
// M4 Webhook: 公開ページ再生成ポートのモック（記録＋制御可能）。
// ルート→ジョブ→再生成の写像を、非同期ジョブを検証するために使う。
// ============================================================================

use std::future::Future;
use std::pin::Pin;

use crate::ports::{PortError, RebuildPort};

/// テスト用の再生成ポート。
///
/// - `calls`: 呼出の記録（projectId）。
/// - `fail`: true のとき再生成を失敗させる。
#[derive(Clone, Default)]
pub struct MockRebuild {
    pub calls: Arc<Mutex<Vec<String>>>,
    pub fail: Arc<Mutex<bool>>,
}

impl MockRebuild {
    pub fn new() -> Self {
        Self::default()
    }

    /// 再生成を失敗させる。
    pub fn failing(self) -> Self {
        *self.fail.lock().unwrap() = true;
        self
    }

    /// 記録された呼出を返す。
    pub fn recorded(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl RebuildPort for MockRebuild {
    fn rebuild_public_page(
        &self,
        project_id: &str,
        _repo: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), PortError>> + Send + '_>> {
        let project_id = project_id.to_string();
        let calls = self.calls.clone();
        let fail = self.fail.clone();
        Box::pin(async move {
            calls.lock().unwrap().push(project_id);
            if *fail.lock().unwrap() {
                return Err(PortError::Unavailable);
            }
            Ok(())
        })
    }
}
