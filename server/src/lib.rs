//! novnow-hub バックエンドのライブラリ本体。
//!
//! エントリポイント（`main`）とライブラリ（テスト用）を分ける。

pub mod adapters;
pub mod app_state;
pub mod config;
pub mod error;
pub mod middleware;
pub mod multipart;
pub mod notifications;
pub mod ports;
pub mod project_index;
pub mod registry;
pub mod routes;
pub mod session;
pub mod verify;
pub mod zip_import;

use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::adapters::github::client::{GitHubClient, OAuthConfig};
use crate::adapters::github::GitHubAdapter;
use crate::app_state::AppState;
use crate::config::Config;
use crate::notifications::NotificationStore;
use crate::registry::ProjectRegistry;
use crate::routes::auth::StateStore;
use crate::session::SessionStore;

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
        );

    Router::new()
        .nest("/api/v1", api_v1)
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
    let github_adapter = Arc::new(GitHubAdapter::new(client, oauth));
    AppState {
        config: config.clone(),
        sessions: Arc::new(SessionStore::new(config.clone())),
        github_adapter,
        state_store: Arc::new(StateStore::new()),
        verifier_store: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        redirect_store: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        projects: Arc::new(ProjectRegistry::new()),
        notifications: Arc::new(NotificationStore::new()),
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


}