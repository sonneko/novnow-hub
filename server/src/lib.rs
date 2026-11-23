//! novnow-hub バックエンドのライブラリ本体。
//!
//! エントリポイント（`main`）とライブラリ（テスト用）を分ける。

pub mod adapters;
pub mod app_state;
pub mod config;
pub mod error;
pub mod middleware;
pub mod multipart;
pub mod ports;
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
        // 公開フロー（POST /projects は require_auth 経由）
        .route(
            "/projects",
            post(routes::projects::create_project).route_layer(
                axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::require_auth,
                ),
            ),
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
    }
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use tower::ServiceExt;

    use crate::ports::Identity;
    use crate::test_support::{
        build_multipart, multipart_request, response_text, test_router, test_state,
    };

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
}
