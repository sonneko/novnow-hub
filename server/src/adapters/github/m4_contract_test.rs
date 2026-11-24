//! M4 コラボレーションポートの契約テスト。
//!
//! ローカルに mock GitHub API サーバー（axum）を立て、GitHub REST API の
//! 応答形状が hub のドメイン型（`Proposal`／`Issue`／`CheckStatus`／
//! `AutomergeState`）へ正しく写像されること、およびアダプターが発する
//! リクエスト（メソッド＋パス＋主要フィールド）が GitHub の REST 契約に
//! 一致することを検証する。GitHub の概念はポートの背後に隠れ、hub 側の
//! 型のみが返ることを確認する。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use http_body_util::BodyExt;

use super::client::{GitHubClient, OAuthConfig, Token};
use super::GitHubAdapter;
use crate::ports::{
    AutomergeConfig, AutomergePort, AutomergeStateKind, CheckStatusPort, CheckState, IssuePort,
    IssueUpdate, ProposalPort, ReviewVerdict,
};

/// 発信されたリクエストの記録。
#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    path: String,
    body: Option<serde_json::Value>,
}

/// mock サーバーが共有する状態。
#[derive(Clone)]
struct MockState {
    recorded: Arc<Mutex<Vec<Recorded>>>,
}

impl MockState {
    fn new() -> Self {
        Self {
            recorded: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

fn json_response(status: StatusCode, value: serde_json::Value) -> Response {
    let mut resp = (status, axum::Json(value)).into_response();
    let headers = resp.headers_mut();
    headers.insert("ETag", "\"repo-etag\"".parse().unwrap());
    resp
}

fn empty_response(status: StatusCode) -> Response {
    status.into_response()
}

async fn read_body(req: Request<Body>) -> (Method, String, Option<serde_json::Value>) {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let body = req
        .into_body()
        .collect()
        .await
        .ok()
        .and_then(|c| String::from_utf8_lossy(&c.to_bytes()).into_owned().parse::<serde_json::Value>().ok());
    (method, path, body)
}

/// コミットのチェックランを返す（GET /repos/{o}/{r}/commits/{ref}/check-runs）。
async fn check_runs(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, _) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body: None,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!({
            "total_count": 1,
            "check_runs": [{
                "name": "novnow-verify",
                "status": "completed",
                "conclusion": "success",
                "html_url": "https://github.com/octo/demo/runs/1"
            }]
        }),
    )
}

/// 评价下の mock サーバーを起動し、ベース URL と記録ハンドルを返す。
async fn start_mock() -> (String, MockState) {
    let state = MockState::new();
    let app = Router::new()
        // リポジトリ情報。
        .route("/repos/{owner}/{repo}", get(repo))
        // Pull Requests。
        .route("/repos/{owner}/{repo}/pulls", get(list_pulls).post(create_pull))
        .route("/repos/{owner}/{repo}/pulls/{n}", get(get_pull).patch(update_pull))
        .route("/repos/{owner}/{repo}/pulls/{n}/files", get(pull_files))
        .route("/repos/{owner}/{repo}/pulls/{n}/reviews", get(pull_reviews).post(post_review))
        .route(
            "/repos/{owner}/{repo}/pulls/{n}/merge",
            axum::routing::put(put_merge).delete(delete_merge).post(post_merge),
        )
        // Issues。
        .route("/repos/{owner}/{repo}/issues", get(list_issues).post(create_issue))
        .route("/repos/{owner}/{repo}/issues/{n}", get(get_issue).patch(update_issue))
        .route(
            "/repos/{owner}/{repo}/issues/{n}/comments",
            get(list_comments).post(post_comment),
        )
        // Check Runs。
        .route("/repos/{owner}/{repo}/commits/{ref}/check-runs", get(check_runs))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), state)
}

async fn repo(_state: State<MockState>) -> Response {
    json_response(
        StatusCode::OK,
        serde_json::json!({"full_name": "octo/demo", "default_branch": "main", "private": false}),
    )
}

async fn create_pull(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, body) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body,
    });
    json_response(
        StatusCode::CREATED,
        serde_json::json!({
            "number": 7,
            "title": "T",
            "body": "B",
            "state": "open",
            "user": {"login": "alice"},
            "base": {"ref": "main"},
            "head": {"ref": "feat"},
            "merged": false,
            "additions": 5,
            "deletions": 2,
            "changed_files": 1,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z"
        }),
    )
}

async fn list_pulls(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, _) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body: None,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!([{
            "number": 7,
            "title": "T",
            "body": "B",
            "state": "open",
            "user": {"login": "alice"},
            "base": {"ref": "main"},
            "head": {"ref": "feat"},
            "merged": false,
            "additions": 5,
            "deletions": 2,
            "changed_files": 1,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z"
        }]),
    )
}

async fn get_pull(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, _) = read_body(req).await;
    let n = path.rsplit('/').next().unwrap_or("0").parse::<i64>().unwrap_or(0);
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body: None,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!({
            "number": n,
            "title": "T",
            "body": "B",
            "state": "open",
            "user": {"login": "alice"},
            "base": {"ref": "main"},
            "head": {"ref": "feat"},
            "merged": false,
            "additions": 5,
            "deletions": 2,
            "changed_files": 1,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z"
        }),
    )
}

async fn update_pull(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, body) = read_body(req).await;
    let n = path.rsplit('/').next().unwrap_or("0").parse::<i64>().unwrap_or(0);
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!({
            "number": n,
            "title": "T",
            "body": "B",
            "state": "closed",
            "user": {"login": "alice"},
            "base": {"ref": "main"},
            "head": {"ref": "feat"},
            "merged": false,
            "additions": 5,
            "deletions": 2,
            "changed_files": 1,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z"
        }),
    )
}

async fn pull_files(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, _) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body: None,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!([{
            "filename": "a.md",
            "status": "modified",
            "additions": 3,
            "deletions": 1,
            "patch": "@@ -1 +1 @@\n-old\n+new"
        }]),
    )
}

async fn pull_reviews(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, _) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body: None,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!([{"state": "APPROVED"}, {"state": "COMMENTED"}]),
    )
}

async fn post_review(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, body) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body,
    });
    empty_response(StatusCode::CREATED)
}

async fn post_merge(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, body) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!({
            "merged": true,
            "sha": "abc123",
            "merged_at": "2026-01-03T00:00:00Z"
        }),
    )
}

async fn put_merge(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, body) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body,
    });
    empty_response(StatusCode::NO_CONTENT)
}

async fn delete_merge(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, _) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body: None,
    });
    empty_response(StatusCode::NO_CONTENT)
}

async fn create_issue(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, body) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body,
    });
    json_response(
        StatusCode::CREATED,
        serde_json::json!({
            "number": 3,
            "title": "bug",
            "body": "x",
            "state": "open",
            "labels": [{"name": "bug"}],
            "user": {"login": "alice"},
            "assignee": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }),
    )
}

async fn list_issues(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, _) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body: None,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!([
            {"number": 3, "title": "bug", "body": "x", "state": "open",
             "labels": [{"name": "bug"}], "user": {"login": "alice"}, "assignee": null,
             "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"},
            {"number": 4, "title": "PR 混入", "body": "", "state": "open",
             "labels": [], "user": {"login": "alice"}, "assignee": null,
             "pull_request": {"url": "https://api.github.com/repos/octo/demo/pulls/4"},
             "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"}
        ]),
    )
}

async fn get_issue(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, _) = read_body(req).await;
    let n = path.rsplit('/').next().unwrap_or("0").parse::<i64>().unwrap_or(0);
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body: None,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!({
            "number": n,
            "title": "bug",
            "body": "x",
            "state": "open",
            "labels": [{"name": "bug"}],
            "user": {"login": "alice"},
            "assignee": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }),
    )
}

async fn update_issue(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, body) = read_body(req).await;
    let n = path.rsplit('/').next().unwrap_or("0").parse::<i64>().unwrap_or(0);
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!({
            "number": n,
            "title": "bug",
            "body": "x",
            "state": "closed",
            "labels": [{"name": "bug"}],
            "user": {"login": "alice"},
            "assignee": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }),
    )
}

async fn list_comments(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, _) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body: None,
    });
    json_response(
        StatusCode::OK,
        serde_json::json!([{
            "id": 1,
            "user": {"login": "bob"},
            "body": "hi",
            "path": null,
            "line": null,
            "created_at": "2026-01-01T00:00:00Z"
        }]),
    )
}

async fn post_comment(State(state): State<MockState>, req: Request<Body>) -> Response {
    let (method, path, body) = read_body(req).await;
    state.recorded.lock().unwrap().push(Recorded {
        method: method.to_string(),
        path,
        body,
    });
    json_response(
        StatusCode::CREATED,
        serde_json::json!({
            "id": 2,
            "user": {"login": "bob"},
            "body": "msg",
            "path": null,
            "line": null,
            "created_at": "2026-01-01T00:00:00Z"
        }),
    )
}

/// mock に向かうアダプターを組み立てる。
fn build_adapter(base_url: &str) -> GitHubAdapter {
    let client = GitHubClient::new(base_url, base_url, Duration::from_secs(5), Duration::from_secs(10));
    let oauth = OAuthConfig {
        client_id: "cid".into(),
        client_secret: "secret".into(),
        redirect_uri: "http://localhost/cb".into(),
    };
    let adapter = GitHubAdapter::new(client, oauth);
    adapter.set_token(Token::new("test-token".into()));
    adapter
}

fn last(state: &MockState) -> Recorded {
    state.recorded.lock().unwrap().last().unwrap().clone()
}

fn has_request(state: &MockState, method: &str, path_suffix: &str) -> bool {
    state
        .recorded
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.method == method && r.path.ends_with(path_suffix))
}

fn find_request(state: &MockState, method: &str, path_suffix: &str) -> Recorded {
    state
        .recorded
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.method == method && r.path.ends_with(path_suffix))
        .expect("記録されたリクエストが存在する")
        .clone()
}

#[tokio::test]
async fn proposal_port_maps_github_pr() {
    let (base_url, state) = start_mock().await;
    let adapter = build_adapter(&base_url);

    // open_proposal → POST /repos/{o}/{r}/pulls。
    let proposal = adapter
        .open_proposal("octo/demo", "T", "B", "feat", "main")
        .await
        .expect("open_proposal が成功する");
    assert_eq!(proposal.number, 7);
    assert_eq!(proposal.title, "T");
    assert_eq!(proposal.author_handle, "alice");
    assert_eq!(proposal.base_ref, "main");
    assert_eq!(proposal.head_ref, "feat");
    assert_eq!(proposal.diff_stats.additions, 5);
    assert_eq!(proposal.diff_stats.deletions, 2);
    assert_eq!(proposal.diff_stats.changed_files, 1);
    // レビューから承認数を集計（APPROVED 1 件）。
    assert_eq!(proposal.approval_count, 1);
    // open_proposal は create の後にレビュー一覧を取得するため、POST を検出する。
    let rec = find_request(&state, "POST", "/repos/octo/demo/pulls");
    assert_eq!(rec.path, "/repos/octo/demo/pulls");
    let body = rec.body.unwrap();
    assert_eq!(body["title"], "T");
    assert_eq!(body["head"], "feat");
    assert_eq!(body["base"], "main");

    // get_proposal → GET /repos/{o}/{r}/pulls/7。
    let fetched = adapter.get_proposal("octo/demo", 7).await.expect("get_proposal");
    assert_eq!(fetched.number, 7);
    assert!(has_request(&state, "GET", "/repos/octo/demo/pulls/7"));

    // diff_proposal → GET .../pulls/7/files。
    let diff = adapter.diff_proposal("octo/demo", 7).await.expect("diff_proposal");
    assert_eq!(diff.number, 7);
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "a.md");
    assert!(diff.files[0].hunks.iter().any(|h| h.contains("+new")));
    assert!(has_request(&state, "GET", "/repos/octo/demo/pulls/7/files"));

    // comment_proposal → POST .../issues/7/comments（PR は Issue として扱う）。
    let comment = adapter
        .comment_proposal("octo/demo", 7, "msg", None, None)
        .await
        .expect("comment_proposal");
    assert_eq!(comment.author_handle, "bob");
    assert!(has_request(&state, "POST", "/repos/octo/demo/issues/7/comments"));

    // review_proposal → POST .../pulls/7/reviews。
    adapter
        .review_proposal("octo/demo", 7, ReviewVerdict::Approve, None)
        .await
        .expect("review_proposal");
    let rec = find_request(&state, "POST", "/repos/octo/demo/pulls/7/reviews");
    assert_eq!(rec.body.unwrap()["event"], "APPROVE");

    // merge_proposal → POST .../pulls/7/merge。
    let merged = adapter.merge_proposal("octo/demo", 7).await.expect("merge_proposal");
    assert!(merged.merged);
    assert_eq!(merged.sha, "abc123");
    assert!(merged.merged_at.is_some());
    let rec = find_request(&state, "POST", "/repos/octo/demo/pulls/7/merge");
    assert!(rec.path.ends_with("/repos/octo/demo/pulls/7/merge"));

    // close_proposal → PATCH .../pulls/7 で state=closed。
    adapter.close_proposal("octo/demo", 7).await.expect("close_proposal");
    let rec = find_request(&state, "PATCH", "/repos/octo/demo/pulls/7");
    assert_eq!(rec.body.unwrap()["state"], "closed");
}

#[tokio::test]
async fn issue_port_maps_github_issue() {
    let (base_url, state) = start_mock().await;
    let adapter = build_adapter(&base_url);

    // open_issue → POST /repos/{o}/{r}/issues。
    let issue = adapter
        .open_issue("octo/demo", "bug", "x", &["bug".to_string()])
        .await
        .expect("open_issue");
    assert_eq!(issue.number, 3);
    assert_eq!(issue.title, "bug");
    assert_eq!(issue.labels, vec!["bug".to_string()]);
    assert_eq!(issue.author_handle, "alice");
    let rec = last(&state);
    assert_eq!(rec.method, "POST");
    assert!(rec.path.ends_with("/repos/octo/demo/issues"));
    assert_eq!(rec.body.unwrap()["labels"][0], "bug");

    // list_issues → GET .../issues。PR が混入していても除外される。
    let issues = adapter.list_issues("octo/demo", None, None).await.expect("list_issues");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].number, 3);
    assert!(has_request(&state, "GET", "/repos/octo/demo/issues"));

    // update_issue → PATCH .../issues/3。
    adapter
        .update_issue(
            "octo/demo",
            3,
            &IssueUpdate {
                title: Some("bug2".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_issue");
    let rec = last(&state);
    assert_eq!(rec.method, "PATCH");
    assert!(rec.path.ends_with("/repos/octo/demo/issues/3"));
    assert_eq!(rec.body.unwrap()["title"], "bug2");

    // comment_issue → POST .../issues/3/comments。
    let comment = adapter
        .comment_issue("octo/demo", 3, "msg")
        .await
        .expect("comment_issue");
    assert_eq!(comment.body, "msg");
    assert!(has_request(&state, "POST", "/repos/octo/demo/issues/3/comments"));

    // close_issue → PATCH .../issues/3 で state=closed。
    let closed = adapter
        .close_issue("octo/demo", 3, crate::ports::IssueState::Closed)
        .await
        .expect("close_issue");
    assert_eq!(closed.state, crate::ports::IssueState::Closed);
    let rec = last(&state);
    assert_eq!(rec.body.unwrap()["state"], "closed");

    // list_issue_comments → GET .../issues/3/comments。
    let comments = adapter
        .list_issue_comments("octo/demo", 3)
        .await
        .expect("list_issue_comments");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].author_handle, "bob");
    assert!(has_request(&state, "GET", "/repos/octo/demo/issues/3/comments"));
}

#[tokio::test]
async fn check_status_port_maps_check_runs() {
    let (base_url, state) = start_mock().await;
    let adapter = build_adapter(&base_url);

    let checks = adapter
        .get_check_status("octo/demo", "feat")
        .await
        .expect("get_check_status");
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].name, "novnow-verify");
    assert_eq!(checks[0].state, CheckState::Success);
    assert!(checks[0].details_url.is_some());

    let rec = last(&state);
    assert_eq!(rec.method, "GET");
    assert_eq!(rec.path, "/repos/octo/demo/commits/feat/check-runs");
}

#[tokio::test]
async fn automerge_port_reserve_and_cancel() {
    let (base_url, state) = start_mock().await;
    let adapter = build_adapter(&base_url);

    // reserve → PUT /repos/{o}/{r}/pulls/7/merge。
    let config = AutomergeConfig {
        required_approvals: 2,
        required_checks: vec!["novnow-verify".to_string()],
        require_no_conflicts: true,
    };
    let st = adapter
        .reserve_automerge("octo/demo", 7, &config)
        .await
        .expect("reserve_automerge");
    assert_eq!(st.state, AutomergeStateKind::Reserved);
    assert_eq!(st.required_approvals, 2);
    assert_eq!(st.required_checks, vec!["novnow-verify".to_string()]);
    assert!(!st.etag.is_empty());
    let rec = last(&state);
    assert_eq!(rec.method, "PUT");
    assert!(rec.path.ends_with("/repos/octo/demo/pulls/7/merge"));
    let body = rec.body.unwrap();
    assert_eq!(body["required_approvals"], 2);
    assert_eq!(body["required_status_checks"]["contexts"][0], "novnow-verify");

    // cancel → DELETE .../pulls/7/merge。
    adapter.cancel_automerge("octo/demo", 7).await.expect("cancel_automerge");
    let rec = last(&state);
    assert_eq!(rec.method, "DELETE");
    assert!(rec.path.ends_with("/repos/octo/demo/pulls/7/merge"));

    // get_state は予約済みとして返す。
    let st = adapter
        .get_automerge_state("octo/demo", 7)
        .await
        .expect("get_automerge_state");
    assert_eq!(st.state, AutomergeStateKind::Reserved);
}

#[tokio::test]
async fn token_missing_is_permission_denied() {
    let (base_url, _state) = start_mock().await;
    let client = GitHubClient::new(&base_url, &base_url, Duration::from_secs(5), Duration::from_secs(10));
    let adapter = GitHubAdapter::new(
        client,
        OAuthConfig {
            client_id: "cid".into(),
            client_secret: "secret".into(),
            redirect_uri: "http://localhost/cb".into(),
        },
    );
    let err = adapter
        .get_proposal("octo/demo", 7)
        .await
        .expect_err("トークン未設定は失敗する");
    assert!(matches!(err, crate::ports::PortError::PermissionDenied));
}

