//! novnow-hub バックエンドのライブラリ本体。
//!
//! エントリポイント（`main`）とライブラリ（テスト用）を分ける。

pub mod adapters;
pub mod app_state;
pub mod config;
pub mod derivatives;
pub mod error;
pub mod jobs;
pub mod middleware;
pub mod multipart;
pub mod notifications;
pub mod ports;
pub mod project_index;
pub mod registry;
pub mod routes;
pub mod session;
pub mod verify;
pub mod webhook;
pub mod zip_import;

use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::adapters::github::client::{GitHubClient, OAuthConfig, Token};
use crate::adapters::github::{DerivationAdapter, GitHubAdapter};
use crate::app_state::AppState;
use crate::config::Config;
use crate::derivatives::DerivativeStore;
use crate::notifications::NotificationStore;
use crate::registry::ProjectRegistry;
use crate::routes::auth::StateStore;
use crate::session::SessionStore;
use crate::webhook::{LoggingRebuild, WebhookStore};

/// 全ルーターを組み立てる。
///
/// `/api/v1` をネストルータで管理し、`POST /projects` には `require_auth`
/// ミドルウェアを適用する。`main` とテストの両方で同じ構成を使う。
pub fn build_router(state: AppState) -> Router {
    let api_v1 = Router::new()
        // メタ
        .route("/meta", get(routes::meta::get_meta))
        // 認証（OAuth + PKCE）
        .route("/auth/github/start", get(routes::auth::start))
        .route("/auth/github/callback", get(routes::auth::callback))
        .route("/auth/refresh", post(routes::auth::refresh))
        .route("/auth/logout", post(routes::auth::logout))
        .route("/auth/account", delete(routes::auth::account))
        // プロジェクト一覧・検索（公開。仕様 §4.1・§4.2）
        .route("/projects", get(routes::projects::list_projects))
        .route("/search/projects", get(routes::projects::search_projects))
        // 公開フロー（POST /projects は require_auth 経由）
        .merge(
            Router::new().route(
                "/projects",
                post(routes::projects::create_project).route_layer(
                    axum::middleware::from_fn_with_state(
                        state.clone(),
                        middleware::require_auth,
                    ),
                ),
            ),
        )
        // M4 コラボレーション：変更提案・課題・自動マージ（公開読み取り）。
        .merge(
            Router::new()
                .route(
                    "/projects/{projectId}/proposals",
                    get(routes::collab::list_proposals),
                )
                .route(
                    "/projects/{projectId}/proposals/{number}",
                    get(routes::collab::get_proposal),
                )
                .route(
                    "/projects/{projectId}/proposals/{number}/diff",
                    get(routes::collab::get_proposal_diff),
                )
                .route(
                    "/projects/{projectId}/proposals/{number}/automerge",
                    get(routes::collab::get_automerge_state),
                )
                .route("/projects/{projectId}/issues", get(routes::collab::list_issues))
                .route(
                    "/projects/{projectId}/issues/{number}",
                    get(routes::collab::get_issue),
                ),
        )
        // M4 コラボレーション：書き込み系（認証必須）。
        .merge(
            Router::new()
                .route(
                    "/projects/{projectId}/proposals",
                    post(routes::collab::open_proposal),
                )
                .route(
                    "/projects/{projectId}/proposals/{number}/comments",
                    get(routes::collab::list_proposal_comments)
                        .post(routes::collab::comment_proposal),
                )
                .route(
                    "/projects/{projectId}/proposals/{number}/approve",
                    post(routes::collab::review_proposal),
                )
                .route(
                    "/projects/{projectId}/proposals/{number}/merge",
                    post(routes::collab::merge_proposal),
                )
                .route(
                    "/projects/{projectId}/proposals/{number}/close",
                    post(routes::collab::close_proposal),
                )
                .route(
                    "/projects/{projectId}/proposals/{number}/automerge",
                    axum::routing::put(routes::collab::reserve_automerge)
                        .delete(routes::collab::cancel_automerge),
                )
                .route("/projects/{projectId}/issues", post(routes::collab::open_issue))
                .route(
                    "/projects/{projectId}/issues/{number}",
                    axum::routing::patch(routes::collab::update_issue),
                )
                .route(
                    "/projects/{projectId}/issues/{number}/comments",
                    get(routes::collab::list_issue_comments)
                        .post(routes::collab::comment_issue),
                )
                .route(
                    "/projects/{projectId}/issues/{number}/close",
                    post(routes::collab::close_issue),
                )
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::require_auth,
                )),
        )
        // M4 通知センター（設計 §6.1、認証必須）。
        .merge(
            Router::new()
                .route(
                    "/notifications",
                    get(routes::notifications::list_notifications),
                )
                .route(
                    "/notifications/read",
                    post(routes::notifications::mark_notifications_read),
                )
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::require_auth,
                )),
        )
        // M5 派生：派生ツリーの取得（公開読み取り、契約 `listDerivatives`）。
        .merge(
            Router::new().route(
                "/projects/{projectId}/derivatives",
                get(routes::derivatives::list),
            ),
        )
        // M5 派生：派生作成（認証必須、契約 `deriveProject`）。
        .merge(
            Router::new()
                .route(
                    "/projects/{projectId}/derive",
                    post(routes::derivatives::derive),
                )
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::require_auth,
                )),
        )
        // M5 診断（認証必須、契約 `runDiagnostics`）。
        .merge(
            Router::new()
                .route(
                    "/projects/{projectId}/diagnostics",
                    post(routes::jobs::run_diagnostics),
                )
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::require_auth,
                )),
        )
        // M5 エクスポート（無認証、契約 `createExport`）。
        .merge(
            Router::new().route(
                "/projects/{projectId}/exports",
                post(routes::jobs::create_export),
            ),
        )
        // M5 ジョブ状態取得（認証必須、契約 `getJob`）。
        .merge(
            Router::new()
                .route("/jobs/{jobId}", get(routes::jobs::get_job))
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::require_auth,
                )),
        );

    Router::new()
        .nest("/api/v1", api_v1)
        // Webhook 受信（サーバー内部の受信口。API 契約には追加しない）。
        .route("/internal/webhook", post(routes::webhook::receive))
        .with_state(state)
}

/// テスト・実機検証用に AppState を組み立てる。
pub fn build_app_state(config: Config) -> AppState {
    let client = GitHubClient::new(
        &config.github_api_base,
        &config.github_auth_base,
        std::time::Duration::from_secs(config.read_timeout_secs),
        std::time::Duration::from_secs(config.write_timeout_secs),
    );
    let oauth = OAuthConfig {
        client_id: config.github_oauth_client_id.clone().unwrap_or_default(),
        client_secret: config.github_oauth_client_secret.clone().unwrap_or_default(),
        redirect_uri: config.github_oauth_redirect_uri.clone().unwrap_or_default(),
    };
    // トークンストアは GitHub アダプターと派生アダプターで共有する。
    let token_store = Arc::new(std::sync::RwLock::new(None::<Token>));
    let github_adapter = Arc::new(GitHubAdapter::new(client.clone(), oauth, token_store.clone()));

    let projects = Arc::new(ProjectRegistry::new());
    let derivatives = Arc::new(DerivativeStore::new());
    let notifications = Arc::new(NotificationStore::new());
    let derivation = Arc::new(DerivationAdapter::new(
        client,
        token_store,
        projects.clone(),
        derivatives.clone(),
        notifications.clone(),
    )) as Arc<dyn crate::ports::DerivationPort>;

    // novnow 診断・エクスポートの非同期ジョブ基盤（設計 §4.1）。
    // コンテンツ読み取りは GitHub アダプター（`ContentPort` 実装）を共有する。
    let job_store = Arc::new(crate::jobs::JobStore::new());
    let job_runner = Arc::new(crate::jobs::JobRunner::new(
        github_adapter.clone() as Arc<dyn crate::jobs::JobContentPort>,
        projects.clone(),
    ));
    let jobs = Arc::new(crate::jobs::JobManager::new(
        job_store,
        job_runner,
        notifications.clone(),
    )) as Arc<dyn crate::ports::JobPort>;

    AppState {
        config: config.clone(),
        sessions: Arc::new(SessionStore::new(config.clone())),
        github_adapter,
        state_store: Arc::new(StateStore::new()),
        verifier_store: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        redirect_store: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        projects,
        notifications,
        webhook: Arc::new(WebhookStore::new()),
        rebuild: Arc::new(LoggingRebuild),
        derivatives,
        derivation,
        jobs,
    }
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::app_state::AppState;
    use crate::ports::Identity;
    use crate::test_support::{
        build_multipart, get_request, multipart_request, response_text, test_router, test_state,
    };

    fn authed_get(path: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(path)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    }

    fn authed_post(path: &str, token: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(path)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    /// novnow 検証を通る最小 zip を生成する（stored 方式）。
    fn valid_zip() -> Vec<u8> {
        crate::zip_import::tests::make_zip(&[
            ("project.toml", b"schema_version = 2\n", b"schema_version = 2\n", 0),
            ("characters.md", b"# char\n", b"# char\n", 0),
            ("words.md", b"# words\n", b"# words\n", 0),
            ("materials/index.json", b"{}\n", b"{}\n", 0),
            ("plots/all.cnov", b"plot\n", b"plot\n", 0),
            ("contents/01.cnov", b"body\n", b"body\n", 0),
        ])
    }

    /// 検証失敗となる zip（必須ファイルを欠落）。
    fn invalid_zip() -> Vec<u8> {
        crate::zip_import::tests::make_zip(&[
            ("project.toml", b"schema_version = 2\n", b"schema_version = 2\n", 0),
        ])
    }

    fn publish_body(zip: &[u8]) -> Vec<u8> {
        build_multipart(
            "----testboundary7MA4YWxkTrZu0gW",
            &[
                ("title", None, "月明かりの庭".as_bytes()),
                ("license", None, b"cc-by"),
                ("archive", Some("book.zip"), zip),
            ],
        )
    }

    #[tokio::test]
    async fn post_projects_rejects_without_auth() {
        let router = test_router(test_state());
        let req = multipart_request("/api/v1/projects", &publish_body(&valid_zip()), None);
        let resp = router.oneshot(req).await.unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_projects_rejects_invalid_zip_before_storage() {
        // 認証済みでも、novnow 検証失敗は保管単位作成の前に 400 で返る。
        let state = test_state();
        // Identity を紐付けたセッションを作成し、アクセストークンを取得。
        let identity = Identity {
            id: "u-1".into(),
            handle: "sora".into(),
            display_name: "sora".into(),
            avatar_url: None,
        };
        let session = state.sessions.create_with_identity("u-1", Some(identity));

        let router = test_router(state);
        let req = multipart_request(
            "/api/v1/projects",
            &publish_body(&invalid_zip()),
            Some(&session.session_id),
        );
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        // novnow 検証失敗 → 400 (VerificationFailed)。保管基盤は呼ばれない。
        assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
        assert!(body.contains("verification_failed"), "body={body}");
    }

    #[tokio::test]
    async fn post_projects_valid_zip_passes_verification_and_reaches_storage() {
        // 有効 zip は novnow 検証を通過し、保管単位作成（GitHub アダプター）へ進む。
        // 沙箱には GitHub トークンが無いため、作成は PermissionDenied(403) で終わる
        // が、検証を通過した証拠として 400(verification_failed) ではないことを確認する。
        let state = test_state();
        let identity = Identity {
            id: "u-1".into(),
            handle: "sora".into(),
            display_name: "sora".into(),
            avatar_url: None,
        };
        let session = state.sessions.create_with_identity("u-1", Some(identity));

        let router = test_router(state);
        let req = multipart_request(
            "/api/v1/projects",
            &publish_body(&valid_zip()),
            Some(&session.session_id),
        );
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        // 検証失敗ではない（検証を通過して保管単位作成へ進んだ）。トークン未設定
        // のため GitHub 側で失敗するが、それは novnow 検証とは別の工程。
        assert_ne!(
            status,
            StatusCode::BAD_REQUEST,
            "検証失敗で止まるべきではない: {body}"
        );
        assert!(
            !body.contains("verification_failed"),
            "検証失敗が現れるべきではない: {body}"
        );
    }

    /// 軽量インデックス用のサンプルプロジェクトを登録簿へ登録する。
    fn register_sample(state: &AppState, id: &str, title: &str, license: &str, updated: i64) {
        use crate::ports::ProjectVisibility;
        use crate::registry::{ProjectRecord, DEFAULT_BRANCH};
        state.projects.register(ProjectRecord {
            project_id: id.to_string(),
            title: title.to_string(),
            description: Some(format!("desc {title}")),
            tags: Vec::new(),
            owner_handle: format!("owner-{id}"),
            license: license.to_string(),
            derivative_policy: "allowed".to_string(),
            visibility: ProjectVisibility::Public,
            shell_id: format!("shell-{id}"),
            default_branch: DEFAULT_BRANCH.to_string(),
            etag: format!("etag-{id}"),
            created_at: updated,
            updated_at: updated,
        });
    }

    fn state_with_samples() -> AppState {
        let state = test_state();
        register_sample(&state, "a", "Moonlit Garden", "cc-by", 100);
        register_sample(&state, "b", "Star Voyage", "cc0", 200);
        register_sample(&state, "c", "Deep Forest", "cc-by-sa", 300);
        state
    }

    #[tokio::test]
    async fn list_projects_returns_page_sorted_by_updated() {
        let router = test_router(state_with_samples());
        let req = get_request("/api/v1/projects?limit=10");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        // 更新日時降順 → c, b, a。
        let ids: Vec<&str> = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|it| it["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
        assert!(page["nextCursor"].is_null());
        // camelCase フィールド名で契約に準拠。
        assert_eq!(page["items"][0]["ownerHandle"].as_str().unwrap(), "owner-c");
        assert!(page["items"][0]["publishedUrl"]
            .as_str()
            .unwrap()
            .contains("/reading/?projectId=c"));
    }

    #[tokio::test]
    async fn list_projects_cursor_pagination() {
        let router = test_router(state_with_samples());
        // 1 件ずつ。1 ページ目は c。
        let req = get_request("/api/v1/projects?limit=1");
        let resp = router.clone().oneshot(req).await.unwrap();
        let (_, body) = response_text(resp).await;
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["items"][0]["id"].as_str().unwrap(), "c");
        let cursor = page["nextCursor"].as_str().unwrap().to_string();
        assert!(!cursor.is_empty());

        // 2 ページ目は b。
        let req = get_request(&format!("/api/v1/projects?limit=1&cursor={cursor}"));
        let resp = router.oneshot(req).await.unwrap();
        let (_, body) = response_text(resp).await;
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["items"][0]["id"].as_str().unwrap(), "b");
    }

    #[tokio::test]
    async fn list_projects_rejects_invalid_sort() {
        let router = test_router(state_with_samples());
        let req = get_request("/api/v1/projects?sort=bogus");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("validation_failed"));
    }

    #[tokio::test]
    async fn list_projects_rejects_invalid_limit() {
        let router = test_router(state_with_samples());
        let req = get_request("/api/v1/projects?limit=0");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("validation_failed"));
    }

    #[tokio::test]
    async fn search_projects_by_keyword() {
        let router = test_router(state_with_samples());
        let req = get_request("/api/v1/search/projects?q=voyage&limit=10");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        assert_eq!(page["items"][0]["id"].as_str().unwrap(), "b");
    }

    #[tokio::test]
    async fn search_projects_by_license_filter() {
        let router = test_router(state_with_samples());
        // cc-by は a のみ。
        let req = get_request("/api/v1/search/projects?q=garden&license=cc-by&limit=10");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["items"][0]["id"].as_str().unwrap(), "a");
    }

    #[tokio::test]
    async fn search_projects_requires_q() {
        let router = test_router(state_with_samples());
        let req = get_request("/api/v1/search/projects?license=cc-by");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("validation_failed"));
    }

    #[tokio::test]
    async fn search_projects_rejects_invalid_license() {
        let router = test_router(state_with_samples());
        let req = get_request("/api/v1/search/projects?q=garden&license=bogus");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("validation_failed"));
    }
    #[tokio::test]
    async fn notifications_require_auth() {
        let router = test_router(test_state());
        let resp = router
            .oneshot(Request::builder().method("GET").uri("/api/v1/notifications").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn list_and_mark_notifications() {
        use crate::ports::{Identity, NotificationKind, NotificationPort};
        let state = test_state();
        let identity = Identity {
            id: "u-1".into(),
            handle: "sora".into(),
            display_name: "sora".into(),
            avatar_url: None,
        };
        let session = state.sessions.create_with_identity("u-1", Some(identity));
        let token = session.session_id.clone();
        state
            .notifications
            .emit("u-1", NotificationKind::ProposalMerged, Some("prj_1"), Some("octo/demo"), "取り込み", "マージ")
            .await
            .unwrap();
        let router = test_router(state);

        // 一覧（未読 1 件、X-Unread-Count ヘッダ）。
        let resp = router.clone().oneshot(authed_get("/api/v1/notifications", &token)).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        assert_eq!(page["items"][0]["kind"].as_str().unwrap(), "proposal_merged");

        // 全既読化 → 204。
        let resp = router
            .clone()
            .oneshot(authed_post("/api/v1/notifications/read", &token, serde_json::json!({"all": true})))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        // 未読のみ → 空。
        let resp = router
            .oneshot(authed_get("/api/v1/notifications?unreadOnly=true", &token))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        let page: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 0);
    }

    // ============================================================================
    // M4 Webhook: 署名検証と再生成ジョブの統合テスト（設計 §7.1）。
    // ============================================================================

    const TEST_WEBHOOK_SECRET: &str = "test-webhook-secret";

    /// push イベントのペイロード（repository.full_name = shell_id 相当）。
    fn push_payload(full_name: &str) -> Vec<u8> {
        serde_json::json!({
            "action": "created",
            "repository": { "full_name": full_name, "default_branch": "main" },
            "push_changes": [{ "sha": "abc" }]
        })
        .to_string()
        .into_bytes()
    }

    fn webhook_request(body: &[u8], secret: Option<&str>, event: &str, signature: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/internal/webhook")
            .header("content-type", "application/json")
            .header("x-github-event", event);
        if let Some(sig) = signature {
            builder = builder.header("x-hub-signature-256", sig);
        }
        // secret は署名計算にのみ使う（ヘッダには出さない）。
        let _ = secret;
        builder.body(Body::from(body.to_vec())).unwrap()
    }

    fn signed_request(body: &[u8], secret: &str, event: &str) -> Request<Body> {
        let sig = crate::webhook::sign(secret.as_bytes(), body);
        webhook_request(body, Some(secret), event, Some(&sig))
    }

    /// Webhook 受信で署名が不正なら 401。
    #[tokio::test]
    async fn webhook_rejects_invalid_signature() {
        let state = test_state();
        let router = test_router(state);
        let body = push_payload("octo/demo");
        let req = webhook_request(&body, Some(TEST_WEBHOOK_SECRET), "push", Some("sha256=deadbeef"));
        let resp = router.oneshot(req).await.unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    /// 署名ヘッダが無いなら 401。
    #[tokio::test]
    async fn webhook_rejects_missing_signature() {
        let state = test_state();
        let router = test_router(state);
        let body = push_payload("octo/demo");
        let req = webhook_request(&body, Some(TEST_WEBHOOK_SECRET), "push", None);
        let resp = router.oneshot(req).await.unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    /// 正しい **16 進形式**の署名で受信が成功し（202）、再生成ジョブが起票される。
    #[tokio::test]
    async fn webhook_accepts_hex_signature() {
        use crate::test_support::register_test_project;

        let state = test_state();
        register_test_project(&state, "prj_1", "octo/demo");
        let router = test_router(state.clone());
        let body = push_payload("octo/demo");
        // sign は 16 進形式（sha256=<hex>）を生成する。
        let hex_sig = crate::webhook::sign(TEST_WEBHOOK_SECRET.as_bytes(), &body);
        assert!(hex_sig.starts_with("sha256="));
        let req = webhook_request(&body, Some(TEST_WEBHOOK_SECRET), "push", Some(&hex_sig));
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::ACCEPTED, "body={body}");
        let job_id = serde_json::from_str::<serde_json::Value>(&body)
            .unwrap()["jobId"]
            .as_str()
            .unwrap()
            .to_string();
        // 再生成ジョブが起票されている（受信と同時に Pending で登録される）。
        let job = state.webhook.get(&job_id).expect("ジョブが起票されている");
        assert_eq!(job.project_id, "prj_1");
    }

    /// **base64 形式**の署名は受け付けず 401 になる（GitHub は 16 進ダイジェスト）。
    #[tokio::test]
    async fn webhook_rejects_base64_signature() {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;

        let state = test_state();
        let router = test_router(state);
        let body = push_payload("octo/demo");

        // 正しい署名（hex）を算出し、そのダイジェストを base64 形式に替える。
        let hex_sig = crate::webhook::sign(TEST_WEBHOOK_SECRET.as_bytes(), &body);
        let hex = hex_sig.strip_prefix("sha256=").unwrap();
        let bytes: Vec<u8> = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        let base64_sig = format!("sha256={}", STANDARD.encode(bytes));

        let req = webhook_request(&body, Some(TEST_WEBHOOK_SECRET), "push", Some(&base64_sig));
        let resp = router.oneshot(req).await.unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    /// 署名検証成功后、登録済みリポジトリの公開ページ再生成ジョブが起票される。
    #[tokio::test]
    async fn webhook_accepts_valid_signature_and_enqueues_rebuild() {
        use crate::test_support::{register_test_project, MockRebuild};
        use crate::webhook::WebhookJobStatus;

        let mut state = test_state();
        register_test_project(&state, "prj_1", "octo/demo");
        let mock = MockRebuild::new();
        state.rebuild = std::sync::Arc::new(mock.clone());

        // router は state を消費するので、共有する Arc を残すために clone する。
        let router = test_router(state.clone());
        let body = push_payload("octo/demo");
        let req = signed_request(&body, TEST_WEBHOOK_SECRET, "push");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::ACCEPTED, "body={body}");
        let body_json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let job_id = body_json["jobId"].as_str().unwrap().to_string();
        assert!(!job_id.is_empty());

        // 非同期ジョブが再生成を実行し、成功に至るまで待つ。
        let mut status = None;
        for _ in 0..200 {
            if let Some(job) = state.webhook.get(&job_id) {
                status = Some(job.status);
                if job.status == WebhookJobStatus::Succeeded {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(status, Some(WebhookJobStatus::Succeeded));
        assert_eq!(mock.recorded(), vec!["prj_1".to_string()]);
    }

    /// 登録簿に無いリポジトリは 404。
    #[tokio::test]
    async fn webhook_rejects_unknown_project() {
        let state = test_state();
        let router = test_router(state);
        let body = push_payload("unknown/repo");
        let req = signed_request(&body, TEST_WEBHOOK_SECRET, "push");
        let resp = router.oneshot(req).await.unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// 再生成対象外（非 push）のイベントは受信自体は成功し、ジョブは起票されない。
    #[tokio::test]
    async fn webhook_ignores_non_push_event() {
        use crate::test_support::register_test_project;
        let state = test_state();
        register_test_project(&state, "prj_1", "octo/demo");
        let router = test_router(state.clone());
        let body = push_payload("octo/demo");
        let req = signed_request(&body, TEST_WEBHOOK_SECRET, "ping");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("skipped"));
        assert!(state.webhook.list().is_empty());
    }

    /// 署名検証成功→再生成ジョブ起票→失敗→リトライの導線を検証する。
    #[tokio::test]
    async fn webhook_rebuild_failure_is_retryable() {
        use crate::test_support::{register_test_project, MockRebuild};
        use crate::webhook::{run_job, WebhookJobStatus};

        let mut state = test_state();
        register_test_project(&state, "prj_1", "octo/demo");
        let mock = MockRebuild::new().failing();
        state.rebuild = std::sync::Arc::new(mock.clone());

        // 署名検証付き受信でジョブを起票。
        let router = test_router(state.clone());
        let body = push_payload("octo/demo");
        let req = signed_request(&body, TEST_WEBHOOK_SECRET, "push");
        let resp = router.oneshot(req).await.unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let job_id = serde_json::from_str::<serde_json::Value>(&body)
            .unwrap()["jobId"]
            .as_str()
            .unwrap()
            .to_string();

        // 非同期ジョブが失敗するまで待つ。
        let mut job = None;
        for _ in 0..200 {
            if let Some(j) = state.webhook.get(&job_id) {
                if j.status == WebhookJobStatus::Failed {
                    job = Some(j);
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(job.as_ref().map(|j| j.status), Some(WebhookJobStatus::Failed));
        assert!(job.as_ref().unwrap().error.is_some());

        // 失敗ジョブを手動リトライ。再生成が成功するようにモックを直す。
        *mock.fail.lock().unwrap() = false;
        let retried = state.webhook.retry(&job_id).unwrap();
        assert_eq!(retried.status, WebhookJobStatus::Pending);
        assert_eq!(retried.attempts, 2);

        // リトライしたジョブを再実行すると成功する。
        let job = state.webhook.get(&job_id).unwrap();
        run_job(
            state.webhook.as_ref(),
            state.rebuild.as_ref(),
            job.job_id.clone(),
            job.project_id.clone(),
            job.repo_full_name.clone(),
        )
        .await;
        assert_eq!(
            state.webhook.get(&job_id).unwrap().status,
            WebhookJobStatus::Succeeded
        );
    }

    // =========================================================================
    // M5 派生ルートの HTTP 統合テスト（契約 `listDerivatives` / `deriveProject`）。
    // =========================================================================

    /// 指定 policy でテスト用プロジェクトを登録する。
    fn register_derivable_project(state: &AppState, project_id: &str, policy: &str) {
        use crate::ports::ProjectVisibility;
        use crate::registry::{ProjectRecord, DEFAULT_BRANCH};
        state.projects.register(ProjectRecord {
            project_id: project_id.to_string(),
            title: "親作品".to_string(),
            description: None,
            tags: Vec::new(),
            owner_handle: "octo".to_string(),
            license: "cc-by".to_string(),
            derivative_policy: policy.to_string(),
            visibility: ProjectVisibility::Public,
            shell_id: "octo/demo".to_string(),
            default_branch: DEFAULT_BRANCH.to_string(),
            etag: "etag".to_string(),
            created_at: 0,
            updated_at: 0,
        });
    }

    fn test_identity() -> crate::ports::Identity {
        crate::ports::Identity {
            id: "u-1".into(),
            handle: "sora".into(),
            display_name: "sora".into(),
            avatar_url: None,
        }
    }

    /// 派生作成は 202 Accepted + jobId を返す。
    #[tokio::test]
    async fn derive_project_returns_202_with_job_id() {
        let state = test_state();
        register_derivable_project(&state, "prj_parent", "allowed");
        // 派生アダプターが共有するトークンストアにトークンを設定する。
        state.github_adapter.set_token(crate::adapters::github::client::Token::new("test-token".into()));
        let session = state.sessions.create_with_identity("u-1", Some(test_identity()));
        let token = session.session_id.clone();
        let router = test_router(state);

        let resp = router
            .oneshot(authed_post(
                "/api/v1/projects/prj_parent/derive",
                &token,
                serde_json::json!({"title": "派生作品", "description": "説明"}),
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::ACCEPTED, "body={body}");
        let accepted: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!accepted["jobId"].as_str().unwrap_or("").is_empty());
        assert_eq!(accepted["status"], "pending");
    }

    /// 派生元が派生非許可のとき 409 を返す。
    #[tokio::test]
    async fn derive_project_disallowed_returns_409() {
        let state = test_state();
        register_derivable_project(&state, "prj_locked", "disallowed");
        let session = state.sessions.create_with_identity("u-1", Some(test_identity()));
        let token = session.session_id.clone();
        let router = test_router(state);

        let resp = router
            .oneshot(authed_post(
                "/api/v1/projects/prj_locked/derive",
                &token,
                serde_json::json!({"title": "派生作品"}),
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::CONFLICT, "body={body}");
    }

    /// 派生作成は認証を要求する。
    #[tokio::test]
    async fn derive_project_requires_auth() {
        let state = test_state();
        register_derivable_project(&state, "prj_parent", "allowed");
        let router = test_router(state);

        let resp = router
            .oneshot(Request::builder()
                .method("POST")
                .uri("/api/v1/projects/prj_parent/derive")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({"title": "派生作品"}).to_string()))
                .unwrap())
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    /// 派生ツリー取得は公開で配列を返す（全深度）。
    #[tokio::test]
    async fn list_derivatives_returns_tree() {
        let state = test_state();
        register_derivable_project(&state, "prj_parent", "allowed");
        // 直接の子を事前登録。
        state.derivatives.record(
            "prj_parent",
            "child",
            "子作品",
            "bob",
            "cc-by",
            crate::ports::DerivativePolicy::Allowed,
        );
        let router = test_router(state);

        let resp = router
            .oneshot(get_request("/api/v1/projects/prj_parent/derivatives"))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::OK, "body={body}");
        let nodes: serde_json::Value = serde_json::from_str(&body).unwrap();
        let arr = nodes.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["title"], "子作品");
        assert_eq!(arr[0]["depth"], 1);
        assert_eq!(arr[0]["parentId"], "prj_parent");
        assert_eq!(arr[0]["derivativePolicy"], "allowed");
    }

    /// 派生ツリー取得は未知のプロジェクトで 404 を返す。
    #[tokio::test]
    async fn list_derivatives_unknown_project_returns_404() {
        let state = test_state();
        let router = test_router(state);
        let resp = router
            .oneshot(get_request("/api/v1/projects/nope/derivatives"))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // =====================================================================
    // M5 JobPort の HTTP ルート統合テスト（設計 §4.1〜§4.3）。
    // =====================================================================

    /// 診断ジョブの起票は認証必須。認証なしは 401。
    #[tokio::test]
    async fn run_diagnostics_requires_auth() {
        let state = test_state();
        let router = test_router(state);
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/jobs-proj/diagnostics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    /// 診断ジョブの起票は存在しないプロジェクトで 404。
    #[tokio::test]
    async fn run_diagnostics_unknown_project_returns_404() {
        let state = test_state();
        let identity = Identity {
            id: "u-jobs".into(),
            handle: "sora".into(),
            display_name: "sora".into(),
            avatar_url: None,
        };
        let session = state.sessions.create_with_identity("u-jobs", Some(identity));
        let router = test_router(state);
        let resp = router
            .oneshot(authed_post(
                "/api/v1/projects/nope/diagnostics",
                &session.session_id,
                serde_json::json!({}),
            ))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// 診断ジョブの起票は 202 + jobId を返し、ジョブが完了まで遷移する。
    #[tokio::test]
    async fn run_diagnostics_accepts_and_job_completes() {
        let state = test_state();
        register_sample(&state, "jobs-proj", "Jobs Sample", "cc-by", 100);
        let identity = Identity {
            id: "u-jobs".into(),
            handle: "sora".into(),
            display_name: "sora".into(),
            avatar_url: None,
        };
        let session = state.sessions.create_with_identity("u-jobs", Some(identity));
        let router = test_router(state);
        let resp = router
            .clone()
            .oneshot(authed_post(
                "/api/v1/projects/jobs-proj/diagnostics",
                &session.session_id,
                serde_json::json!({}),
            ))
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let accepted: serde_json::Value = serde_json::from_str(&body).unwrap();
        let job_id = accepted["jobId"].as_str().expect("jobId").to_string();
        assert!(!job_id.is_empty());

        // ジョブが完了（外部基盤未接続のため failed か succeeded）するまで待つ。
        let mut final_status = String::new();
        for _ in 0..100 {
            let resp = router
                .clone()
                .oneshot(authed_get(&format!("/api/v1/jobs/{job_id}"), &session.session_id))
                .await
                .unwrap();
            let (_, body) = response_text(resp).await;
            let job: serde_json::Value = serde_json::from_str(&body).unwrap();
            final_status = job["status"].as_str().unwrap_or("").to_string();
            if ["succeeded", "failed"].contains(&final_status.as_str()) {
                assert_eq!(job["id"], job_id);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            ["succeeded", "failed"].contains(&final_status.as_str()),
            "ジョブが完了しなかった: {final_status}"
        );
    }

    /// ジョブ状態取得は存在しないジョブで 404、未認証で 401。
    #[tokio::test]
    async fn get_job_unknown_and_unauth() {
        let state = test_state();
        let identity = Identity {
            id: "u-jobs2".into(),
            handle: "sora".into(),
            display_name: "sora".into(),
            avatar_url: None,
        };
        let session = state.sessions.create_with_identity("u-jobs2", Some(identity));
        let router = test_router(state);

        let resp = router
            .clone()
            .oneshot(authed_get("/api/v1/jobs/nope", &session.session_id))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let resp = router
            .oneshot(get_request("/api/v1/jobs/nope"))
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    /// エクスポートは無認証で受理し、存在しないプロジェクトは 404。
    #[tokio::test]
    async fn create_export_accepts_and_unknown_project_404() {
        let state = test_state();
        register_sample(&state, "jobs-exp", "Export Sample", "cc0", 100);
        let router = test_router(state);

        // 無認証で 202。
        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/jobs-exp/exports")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"format":"markdown"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = response_text(resp).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let accepted: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!accepted["jobId"].as_str().unwrap().is_empty());

        // 存在しないプロジェクトは 404。
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/nope/exports")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"format":"txt"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, _) = response_text(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

}