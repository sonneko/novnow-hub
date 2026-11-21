//! novnow-hub バックエンドのエントリポイント（Rust + Axum）。
//!
//! API 契約 `REF:api/openapi.yaml` にのみ準拠する。契約にない独自拡張はしない。
//! バックエンドの配置箇所は仕様・設計に未定だったため、新規 crate `server/`
//! に配置した（PR 本文に記録する）。

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use novnow_hub_server::adapters::github::client::{GitHubClient, OAuthConfig};
use novnow_hub_server::adapters::github::GitHubAdapter;
use novnow_hub_server::app_state::AppState;
use novnow_hub_server::config::Config;
use novnow_hub_server::routes;
use novnow_hub_server::routes::auth::{self, StateStore};
use novnow_hub_server::session::SessionStore;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::from_env();
    tracing::info!("contract version: {}", config.contract_version);
    tracing::info!("listening on: {}:{}", config.bind_addr, config.port);

    // GitHub クライアントとアダプター（OAuth 交換前にトークンは未設定）。
    let client = GitHubClient::new(
        &config.github_api_base,
        std::time::Duration::from_secs(config.read_timeout_secs),
        std::time::Duration::from_secs(config.write_timeout_secs),
    );
    let oauth = OAuthConfig {
        client_id: config.github_oauth_client_id.clone().unwrap_or_default(),
        redirect_uri: config.github_oauth_redirect_uri.clone().unwrap_or_default(),
    };
    let github_adapter = Arc::new(GitHubAdapter::new(client, oauth));

    let state = AppState {
        config: config.clone(),
        sessions: Arc::new(SessionStore::new(config.clone())),
        github_adapter,
        state_store: Arc::new(StateStore::new()),
        verifier_store: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        redirect_store: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
    };

    let app = build_router(state);
    let listener = TcpListener::bind((config.bind_addr.as_str(), config.port)).await?;
    tracing::info!("server started");
    axum::serve(listener, app).await?;
    Ok(())
}

/// ルーター構成。`/api/v1` をネストルータで管理（設計 §3.1）。
fn build_router(state: AppState) -> Router {
    let api_v1 = Router::new()
        // メタ
        .route("/meta", get(routes::meta::get_meta))
        // 認証（OAuth + PKCE）
        .route("/auth/github/start", get(auth::start))
        .route("/auth/github/callback", get(auth::callback))
        .route("/auth/refresh", post(auth::refresh))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/account", axum::routing::delete(auth::account));

    Router::new()
        .nest("/api/v1", api_v1)
        .with_state(state)
}
