//! 読み取り系（contents／reading／setup／releases）ルートの統合テスト。
//!
//! `test_support::MockGitHub` を保管基盤の代役として起動し、`build_router`
//! 越しに契約 `api/openapi.yaml` の次のエンドポイントを検証する。
//!
//! - `GET /setup/resolve?u=…`
//! - `GET /projects/{projectId}/setup`
//! - `GET /projects/{projectId}/reading`
//! - `GET /projects/{projectId}/tree`
//! - `GET /projects/{projectId}/file`
//! - `POST /projects/{projectId}/releases`（契約に `GET` は無い）
//!
//! 検証は「匿名で 200」「未登録は 404」「不正入力は 400」「応答形状が契約の
//! 必須項目を満たす」「認証必須の経路は未認証で 401」の観点で行う。

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use crate::adapters::github::client::Token;
use crate::app_state::AppState;
use crate::ports::Identity;
use crate::test_support::{
    get_request, register_test_project, response_text, test_router, test_state,
    test_state_with_github_base, MockGitHub,
};

/// novnow 軽量検証を通る最小構成（パス → 内容）。
fn sample_files() -> std::collections::HashMap<String, String> {
    let mut files = std::collections::HashMap::new();
    files.insert(
        "project.toml".to_string(),
        "schema_version = 2\n".to_string(),
    );
    files.insert("characters.md".to_string(), "# 登場人物\n".to_string());
    files.insert("words.md".to_string(), "# 用語\n".to_string());
    files.insert("materials/index.json".to_string(), "{}\n".to_string());
    files.insert("plots/all.cnov".to_string(), "plot\n".to_string());
    files.insert(
        "contents/01.cnov".to_string(),
        "# 第一章\n\n夜の庭は静かでした。\n\n＊ ＊ ＊\n\nおわり。\n".to_string(),
    );
    files.insert(
        "contents/02.cnov".to_string(),
        "# 第二章\n\n朝が来ました。\n".to_string(),
    );
    files
}

/// MockGitHub を起動して state を組み立てる（トークン・プロジェクト登録込み）。
async fn setup(
    project_id: &str,
    files: std::collections::HashMap<String, String>,
) -> (AppState, Router) {
    let mock = MockGitHub::new();
    let base_url = mock.start().await;
    mock.set_files(files);
    let state = test_state_with_github_base(&base_url);
    state
        .github_adapter
        .set_token(Token::new("test-token".into()));
    register_test_project(&state, project_id, "octo/demo");
    let router = test_router(state.clone());
    (state, router)
}

/// 認証済みのセッションを作る（リリース作成用）。
fn session_for(state: &AppState, user_id: &str, handle: &str) -> String {
    state
        .sessions
        .create_with_identity(
            user_id,
            Some(Identity {
                id: user_id.to_string(),
                handle: handle.to_string(),
                display_name: handle.to_string(),
                avatar_url: None,
            }),
        )
        .session_id
}

fn json_request(
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
    token: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder
        .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
        .unwrap()
}

async fn get_json(router: Router, path: &str) -> (StatusCode, serde_json::Value) {
    let resp = router.oneshot(get_request(path)).await.unwrap();
    let (status, body) = response_text(resp).await;
    let value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
    (status, value)
}

// ---------------------------------------------------------------------------
// GET /projects/{projectId}/tree
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tree_is_anonymous_and_contract_shaped() {
    let (_state, router) = setup("prj_t", sample_files()).await;
    let (status, body) = get_json(router, "/api/v1/projects/prj_t/tree").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["entries"].is_array());
    assert_eq!(body["truncated"], serde_json::json!(false));
    let entries = body["entries"].as_array().unwrap();
    // ルート直下には contents ディレクトリと project.toml 等が並ぶ。
    assert!(entries.iter().any(
        |e| e["path"] == serde_json::json!("contents") && e["kind"] == serde_json::json!("dir")
    ));
    assert!(entries
        .iter()
        .any(|e| e["path"] == serde_json::json!("project.toml")
            && e["kind"] == serde_json::json!("file")));
    for entry in entries {
        // 契約 `Tree.entries[]` の必須項目（path・kind・size）。
        assert!(entry["path"].is_string());
        assert!(matches!(entry["kind"].as_str(), Some("file") | Some("dir")));
        assert!(entry["size"].is_u64());
    }
    // 保管基盤の語彙（リポジトリ名）を漏らさない。
    assert!(!body.to_string().contains("octo/demo"));
}

#[tokio::test]
async fn tree_returns_404_for_unknown_project() {
    let state = test_state();
    let router = test_router(state);
    let (status, body) = get_json(router, "/api/v1/projects/nope/tree").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], serde_json::json!("not_found"));
}

// ---------------------------------------------------------------------------
// GET /projects/{projectId}/file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn file_is_anonymous_and_returns_utf8_content() {
    let (_state, router) = setup("prj_f", sample_files()).await;
    let (status, body) =
        get_json(router, "/api/v1/projects/prj_f/file?path=contents/01.cnov").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["path"], serde_json::json!("contents/01.cnov"));
    assert_eq!(body["encoding"], serde_json::json!("utf8"));
    assert!(body["etag"]
        .as_str()
        .map(|e| !e.is_empty())
        .unwrap_or(false));
    assert!(body["content"]
        .as_str()
        .map(|c| c.contains("夜の庭"))
        .unwrap_or(false));
    // utf8 のときは data／mime を返さない（契約の説明に従う）。
    assert!(body.get("data").is_none());
    assert!(body.get("mime").is_none());
}

#[tokio::test]
async fn file_without_path_is_validation_failed() {
    let (_state, router) = setup("prj_f", sample_files()).await;
    let (status, body) = get_json(router, "/api/v1/projects/prj_f/file").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], serde_json::json!("validation_failed"));
}

#[tokio::test]
async fn file_with_unknown_path_is_not_found() {
    let (_state, router) = setup("prj_f", sample_files()).await;
    let (status, body) = get_json(router, "/api/v1/projects/prj_f/file?path=missing.md").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
    assert_eq!(body["code"], serde_json::json!("not_found"));
}

// ---------------------------------------------------------------------------
// GET /projects/{projectId}/reading
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reading_is_anonymous_and_contract_shaped() {
    let (_state, router) = setup("prj_r", sample_files()).await;
    let (status, body) = get_json(router, "/api/v1/projects/prj_r/reading").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["projectId"], serde_json::json!("prj_r"));
    assert_eq!(body["title"], serde_json::json!("Test Project"));
    assert!(body["etag"]
        .as_str()
        .map(|e| e.len() == 64)
        .unwrap_or(false));
    let blocks = body["blocks"].as_array().unwrap();
    assert!(!blocks.is_empty());
    for block in blocks {
        assert!(
            matches!(
                block["kind"].as_str(),
                Some("heading") | Some("paragraph") | Some("dialogue") | Some("scene_break")
            ),
            "kind={}",
            block["kind"]
        );
        assert!(block["text"].is_string());
    }
    // contents/01.cnov の見出しと本文が含まれる。
    assert!(blocks
        .iter()
        .any(|b| b["kind"] == serde_json::json!("heading")
            && b["text"] == serde_json::json!("第一章")));
    assert!(blocks
        .iter()
        .any(|b| b["kind"] == serde_json::json!("scene_break")));
    assert!(body.to_string().contains("夜の庭"));
}

#[tokio::test]
async fn reading_etag_is_stable_and_content_derived() {
    let (_state, router) = setup("prj_r", sample_files()).await;
    let (_, first) = get_json(router.clone(), "/api/v1/projects/prj_r/reading").await;
    let (_, second) = get_json(router, "/api/v1/projects/prj_r/reading").await;
    assert_eq!(first["etag"], second["etag"]);
}

#[tokio::test]
async fn reading_returns_404_for_unknown_project() {
    let state = test_state();
    let router = test_router(state);
    let (status, _) = get_json(router, "/api/v1/projects/nope/reading").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// GET /projects/{projectId}/setup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn setup_is_anonymous_and_contract_shaped() {
    let (_state, router) = setup("prj_s", sample_files()).await;
    let (status, body) = get_json(router, "/api/v1/projects/prj_s/setup").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["projectId"], serde_json::json!("prj_s"));
    assert_eq!(body["slug"], serde_json::json!("test-project"));
    assert!(body["snapshotVersion"].is_string());
    assert!(matches!(
        body["acquisition"]["kind"].as_str(),
        Some("archive") | Some("git")
    ));
    assert!(body["acquisition"]["url"]
        .as_str()
        .map(|u| u.starts_with("http"))
        .unwrap_or(false));
    assert_eq!(body["novnow"]["minVersion"], serde_json::json!("1.0.0"));
    // 保管基盤の語彙（リポジトリ名）を漏らさない。
    assert!(!body.to_string().contains("octo/demo"));
}

#[tokio::test]
async fn setup_returns_404_for_unknown_project() {
    let state = test_state();
    let router = test_router(state);
    let (status, _) = get_json(router, "/api/v1/projects/nope/setup").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// GET /setup/resolve
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_accepts_setup_path_url() {
    let (_state, router) = setup("prj_x", sample_files()).await;
    let path = "/api/v1/setup/resolve?u=https%3A%2F%2Fhub.example.test%2Fprojects%2Fprj_x%2Fsetup";
    let (status, body) = get_json(router, path).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["projectId"], serde_json::json!("prj_x"));
    assert!(body["acquisition"]["url"].is_string());
}

#[tokio::test]
async fn resolve_accepts_reading_page_url() {
    let (_state, router) = setup("prj_x", sample_files()).await;
    let path =
        "/api/v1/setup/resolve?u=https%3A%2F%2Fhub.example.test%2Freading%2F%3FprojectId%3Dprj_x";
    let (status, body) = get_json(router, path).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["projectId"], serde_json::json!("prj_x"));
}

#[tokio::test]
async fn resolve_rejects_missing_or_unparsable_url() {
    let (_state, router) = setup("prj_x", sample_files()).await;
    // `u` 欠落。
    let (status, body) = get_json(router.clone(), "/api/v1/setup/resolve").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], serde_json::json!("validation_failed"));
    // プロジェクトを特定できない URL。
    let (status, body) = get_json(
        router,
        "/api/v1/setup/resolve?u=https%3A%2F%2Fexample.com%2Fabout",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], serde_json::json!("validation_failed"));
}

#[tokio::test]
async fn resolve_returns_404_for_unknown_project_id() {
    let (_state, router) = setup("prj_x", sample_files()).await;
    let path =
        "/api/v1/setup/resolve?u=https%3A%2F%2Fhub.example.test%2Fprojects%2Fmissing%2Fsetup";
    let (status, body) = get_json(router, path).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], serde_json::json!("not_found"));
}

// ---------------------------------------------------------------------------
// POST /projects/{projectId}/releases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_release_requires_auth() {
    let (state, router) = setup("prj_rel", sample_files()).await;
    let resp = router
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/projects/prj_rel/releases",
            Some(serde_json::json!({"tag": "v1.0.0", "name": "初版"})),
            None,
        ))
        .await
        .unwrap();
    let (status, _) = response_text(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // 契約に `GET` は無いため一覧取得は存在しない。ルーターは当該パスに
    // `require_auth` を敷いているため、未認証の `GET` は 401 で止まる
    // （認証済みでも 405。契約外の一覧を返さないことを確認する）。
    let (status, _) = get_json(router.clone(), "/api/v1/projects/prj_rel/releases").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let session = session_for(&state, "u-rel", "octo");
    let resp = router
        .oneshot(json_request(
            Method::GET,
            "/api/v1/projects/prj_rel/releases",
            None,
            Some(&session),
        ))
        .await
        .unwrap();
    let (status, _) = response_text(resp).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn create_release_returns_201_with_contract_shape() {
    let (state, router) = setup("prj_rel", sample_files()).await;
    let session = session_for(&state, "u-rel", "octo");
    let resp = router
        .oneshot(json_request(
            Method::POST,
            "/api/v1/projects/prj_rel/releases",
            Some(
                serde_json::json!({"tag": "v1.0.0", "name": "初版", "notes": "最初の公開版です。"}),
            ),
            Some(&session),
        ))
        .await
        .unwrap();
    let (status, body) = response_text(resp).await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["tag"], serde_json::json!("v1.0.0"));
    assert_eq!(value["name"], serde_json::json!("初版"));
    assert_eq!(value["notes"], serde_json::json!("最初の公開版です。"));
    assert!(value["createdAt"]
        .as_str()
        .map(|s| s.ends_with('Z'))
        .unwrap_or(false));
}

#[tokio::test]
async fn create_release_is_idempotent_and_rejects_conflicting_tag() {
    let (state, router) = setup("prj_rel", sample_files()).await;
    let session = session_for(&state, "u-rel", "octo");
    let body = serde_json::json!({"tag": "v1.0.0", "name": "初版"});
    let resp = router
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/projects/prj_rel/releases",
            Some(body.clone()),
            Some(&session),
        ))
        .await
        .unwrap();
    let (status, _) = response_text(resp).await;
    assert_eq!(status, StatusCode::CREATED);
    // 同じ内容の再送は冪等（201）。
    let resp = router
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/projects/prj_rel/releases",
            Some(body),
            Some(&session),
        ))
        .await
        .unwrap();
    let (status, _) = response_text(resp).await;
    assert_eq!(status, StatusCode::CREATED);
    // 同じタグで別内容は 400（重複）。
    let resp = router
        .oneshot(json_request(
            Method::POST,
            "/api/v1/projects/prj_rel/releases",
            Some(serde_json::json!({"tag": "v1.0.0", "name": "別名"})),
            Some(&session),
        ))
        .await
        .unwrap();
    let (status, body) = response_text(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["code"],
        serde_json::json!("validation_failed")
    );
}

#[tokio::test]
async fn create_release_validates_tag_and_name() {
    let (state, router) = setup("prj_rel", sample_files()).await;
    let session = session_for(&state, "u-rel", "octo");
    for body in [
        serde_json::json!({"tag": "1.0.0", "name": "初版"}),
        serde_json::json!({"tag": "v1.0.0", "name": ""}),
    ] {
        let resp = router
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/projects/prj_rel/releases",
                Some(body),
                Some(&session),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn create_release_forbids_non_owner() {
    let (state, router) = setup("prj_rel", sample_files()).await;
    let session = session_for(&state, "u-other", "someone");
    let resp = router
        .oneshot(json_request(
            Method::POST,
            "/api/v1/projects/prj_rel/releases",
            Some(serde_json::json!({"tag": "v1.0.0", "name": "初版"})),
            Some(&session),
        ))
        .await
        .unwrap();
    let (status, body) = response_text(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["code"],
        serde_json::json!("permission_denied")
    );
}

#[tokio::test]
async fn create_release_returns_404_for_unknown_project() {
    let (state, router) = setup("prj_rel", sample_files()).await;
    let session = session_for(&state, "u-rel", "octo");
    let resp = router
        .oneshot(json_request(
            Method::POST,
            "/api/v1/projects/nope/releases",
            Some(serde_json::json!({"tag": "v1.0.0", "name": "初版"})),
            Some(&session),
        ))
        .await
        .unwrap();
    let (status, _) = response_text(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
