//! M5 派生ポートの契約テスト。
//!
//! ローカルに mock GitHub API サーバー（axum）を立て、派生（Fork）の
//! REST 契約（`POST /repos/{owner}/{repo}/forks` 等のリクエスト形状）と
//! GitHub 応答形状 → hub のドメイン型（`RepoMeta`／`Derivative`／
//! `DeriveAccepted`）への写像を検証する。GitHub の概念はポートの背後に
//! 隠れ、hub 側の型のみが返ることを確認する。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use http_body_util::BodyExt;

use super::client::{GitHubClient, Token};
use super::DerivationAdapter;
use crate::derivatives::DerivativeStore;
use crate::ports::{DeriveRequest, DerivationPort, ProjectVisibility};
use crate::registry::{new_project_id, ProjectRecord, ProjectRegistry};
use crate::notifications::NotificationStore;

/// 発信されたリクエストの記録。
#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    path: String,
}

/// novnow 軽量検証を通る最小スナップショット（パス → 内容）。
fn valid_snapshot_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("project.toml", "schema_version = 1\n"),
        ("characters.md", "# characters\n"),
        ("words.md", "# words\n"),
        ("materials/index.json", "{}"),
        ("plots/all.cnov", "{}"),
    ]
}

#[derive(Clone)]
struct MockState {
    recorded: Arc<Mutex<Vec<Recorded>>>,
    files: Arc<Vec<(&'static str, &'static str)>>,
}

impl MockState {
    fn new() -> Self {
        Self {
            recorded: Arc::new(Mutex::new(Vec::new())),
            files: Arc::new(valid_snapshot_files()),
        }
    }
}

fn json_response(status: StatusCode, value: serde_json::Value) -> Response {
    let mut resp = (status, axum::Json(value)).into_response();
    resp.headers_mut()
        .insert("ETag", "\"fork-etag\"".parse().unwrap());
    resp
}

async fn read_body(req: Request<Body>) -> (Method, String) {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let _ = req.into_body().collect().await;
    (method, path)
}

async fn start_mock() -> (String, MockState) {
    let state = MockState::new();

    async fn user(_s: State<MockState>) -> Response {
        json_response(
            StatusCode::OK,
            serde_json::json!({
                "id": 1, "login": "alice", "name": null, "avatar_url": ""
            }),
        )
    }

    // `GET /repos/{owner}/{repo}`：fork のポーリング確認。
    async fn repo(State(state): State<MockState>, req: Request<Body>) -> Response {
        let (method, path) = read_body(req).await;
        state.recorded.lock().unwrap().push(Recorded {
            method: method.to_string(),
            path,
        });
        json_response(
            StatusCode::OK,
            serde_json::json!({
                "full_name": "alice/demo-fork",
                "default_branch": "main",
                "private": false
            }),
        )
    }

    // `POST /repos/{owner}/{repo}/forks`：fork 作成。
    async fn create_fork(State(state): State<MockState>, req: Request<Body>) -> Response {
        let (method, path) = read_body(req).await;
        state.recorded.lock().unwrap().push(Recorded {
            method: method.to_string(),
            path,
        });
        json_response(
            StatusCode::CREATED,
            serde_json::json!({
                "full_name": "alice/demo-fork",
                "default_branch": "main",
                "private": false
            }),
        )
    }

    // contents 取得（再帰）。パスがディレクトリなら一覧、ファイルなら単体を返す。
    async fn contents(State(state): State<MockState>, req: Request<Body>) -> Response {
        let (method, path) = read_body(req).await;
        state.recorded.lock().unwrap().push(Recorded {
            method: method.to_string(),
            path: path.clone(),
        });
        let rel = path
            .strip_prefix("/repos/alice/demo-fork/contents")
            .unwrap_or("")
            .trim_matches('/')
            .to_string();

        // ファイルなら単体を返す。
        if let Some((_, content)) = state.files.iter().find(|(p, _)| *p == rel) {
            let encoded = STANDARD.encode(content.as_bytes());
            return json_response(
                StatusCode::OK,
                serde_json::json!({"content": encoded, "encoding": "base64", "sha": "s"}),
            );
        }

        // ディレクトリなら子要素の一覧を返す。
        let mut entries: Vec<serde_json::Value> = Vec::new();
        match rel.as_str() {
            "" => {
                entries.push(serde_json::json!({"path": "materials", "sha": "d1", "type": "dir"}));
                entries.push(serde_json::json!({"path": "plots", "sha": "d2", "type": "dir"}));
                entries.push(serde_json::json!({"path": "contents", "sha": "d3", "type": "dir"}));
                for (p, _) in &*state.files {
                    if p.contains('/') {
                        continue;
                    }
                    entries.push(serde_json::json!({"path": p, "sha": "s", "type": "file"}));
                }
            }
            "materials" => entries.push(serde_json::json!({"path": "materials/index.json", "sha": "s", "type": "file"})),
            "plots" => entries.push(serde_json::json!({"path": "plots/all.cnov", "sha": "s", "type": "file"})),
            _ => {}
        }
        json_response(StatusCode::OK, serde_json::Value::Array(entries))
    }

    let app = Router::new()
        .route("/user", get(user))
        .route("/repos/{owner}/{repo}", get(repo))
        .route("/repos/{owner}/{repo}/forks", axum::routing::post(create_fork))
        .route("/repos/{owner}/{repo}/contents", get(contents))
        .route("/repos/{owner}/{repo}/contents/{*path}", get(contents))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), state)
}

/// 共有ストアと token を備えた派生アダプターを組み立てる。
fn build_adapter(
    base_url: &str,
    parent_policy: &str,
) -> (
    DerivationAdapter,
    Arc<DerivativeStore>,
    Arc<ProjectRegistry>,
) {
    let client = GitHubClient::new(base_url, base_url, Duration::from_secs(5), Duration::from_secs(10));
    let token_store = Arc::new(std::sync::RwLock::new(None::<Token>));
    let projects = Arc::new(ProjectRegistry::new());
    let derivatives = Arc::new(DerivativeStore::new());
    let notifications = Arc::new(NotificationStore::new());

    // 親プロジェクトを登録（shell_id = "alice/demo"）。
    projects.register(ProjectRecord {
        project_id: new_project_id(),
        title: "親作品".into(),
        description: None,
        tags: Vec::new(),
        owner_handle: "alice".into(),
        license: "cc-by".into(),
        visibility: ProjectVisibility::Public,
        shell_id: "alice/demo".into(),
        default_branch: "main".into(),
        etag: String::new(),
        created_at: 0,
        updated_at: 0,
        derivative_policy: parent_policy.into(),
    });

    let token = Token::new("test-token".into());
    *token_store.write().unwrap() = Some(token);

    let adapter = DerivationAdapter::new(
        client,
        token_store,
        projects.clone(),
        derivatives.clone(),
        notifications,
    );
    (adapter, derivatives, projects)
}

/// 派生作成は受理時に `DeriveAccepted`（202 + jobId）を返す。
/// Fork 完了は非同期ジョブとして進み、hub の派生レコードが作成される。
#[tokio::test]
async fn create_derivative_accepts_and_records() {
    let (base_url, _state) = start_mock().await;
    let (adapter, derivatives, projects) = build_adapter(&base_url, "allowed");

    let parent_id = projects.all().into_iter().next().unwrap().project_id;
    let accepted = adapter
        .create_derivative(
            &parent_id,
            "user-1",
            "bob",
            &DeriveRequest {
                title: "派生作品".into(),
                description: Some("説明".into()),
                visibility: ProjectVisibility::Private,
            },
        )
        .await
        .expect("派生作成が受理される");

    // 202 + jobId 相当。
    assert!(!accepted.job_id.is_empty());
    assert_eq!(accepted.status, crate::ports::JobStatus::Pending);

    // 非同期ジョブの完了を待って派生レコードが作成される。
    let mut waited = false;
    for _ in 0..50 {
        if !derivatives.all().is_empty() {
            waited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(waited, "派生レコードが作成されなかった");

    let record = derivatives.all().into_iter().next().unwrap();
    assert_eq!(record.parent_id.as_deref(), Some(parent_id.as_str()));
    assert_eq!(record.depth, 1);
    assert_eq!(record.owner_handle, "bob");
}

/// 派生元のライセンスが派生非許可のとき、409（Conflict）を返す。
#[tokio::test]
async fn create_derivative_disallowed_returns_conflict() {
    let (base_url, _state) = start_mock().await;
    let (adapter, _derivatives, projects) = build_adapter(&base_url, "disallowed");

    let parent_id = projects.all().into_iter().next().unwrap().project_id;
    let err = adapter
        .create_derivative(
            &parent_id,
            "user-1",
            "bob",
            &DeriveRequest {
                title: "派生作品".into(),
                description: None,
                visibility: ProjectVisibility::Private,
            },
        )
        .await
        .expect_err("派生不可は失敗する");
    assert!(matches!(err, crate::ports::PortError::Conflict));
}

/// 派生ツリーの取得（`get_derivation_tree`）は派生元連鎖と子孫を返す。
#[tokio::test]
async fn get_derivation_tree_returns_chain_and_descendants() {
    let (base_url, _state) = start_mock().await;
    let (adapter, derivatives, projects) = build_adapter(&base_url, "allowed");

    let parent_id = projects.all().into_iter().next().unwrap().project_id;
    adapter
        .create_derivative(
            &parent_id,
            "u",
            "bob",
            &DeriveRequest {
                title: "子".into(),
                description: None,
                visibility: ProjectVisibility::Private,
            },
        )
        .await
        .expect("受理");

    // ジョブ完了を待つ。
    for _ in 0..50 {
        if !derivatives.all().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let tree = adapter
        .get_derivation_tree(&parent_id)
        .await
        .expect("ツリー取得");
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].parent_id.as_deref(), Some(parent_id.as_str()));
    assert_eq!(tree[0].depth, 1);
}

/// fork 作成の REST 契約：`POST /repos/{owner}/{repo}/forks` を発信する。
#[tokio::test]
async fn create_fork_issues_rest_request() {
    let (base_url, state) = start_mock().await;
    let client = GitHubClient::new(&base_url, &base_url, Duration::from_secs(5), Duration::from_secs(10));
    let token = Token::new("t".into());

    client
        .create_fork(&token, "alice/demo")
        .await
        .expect("fork 作成が成功する");

    let fork_req = state
        .recorded
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.method == "POST" && r.path.ends_with("/forks"))
        .cloned()
        .expect("POST /forks が発信されている");
    assert!(fork_req.path.starts_with("/repos/alice/demo/forks"));
}
