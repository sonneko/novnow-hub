//! 認証ルーター。
//!
//! 契約 `REF:api/openapi.yaml` の 5 パス（`/auth/github/start`・
//! `/auth/github/callback`・`/auth/refresh`・`/auth/logout`・
//! `/auth/account`）と設計書 `REF:docs/designs/api.md` §3 に準拠する。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use rand::Rng;
use serde::Deserialize;

use crate::adapters::github::client::{GitHubClient, OAuthConfig};
use crate::app_state::AppState;
use crate::error::ApiError;
use crate::ports::IdentityPort;
use crate::session::TokenPair;

/// 認可リクエストの状態（state パラメータ）の保存記録。
struct PendingState {
    code_challenge: String,
    expires_at: Instant,
}

/// 認可リクエストの state 保存（10 分 TTL・使い捨て）。設計 §3.2。
#[derive(Debug)]
pub struct StateStore {
    pending: RwLock<HashMap<String, PendingState>>,
}

impl StateStore {
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
        }
    }

    pub fn save(&self, code_challenge: String) -> String {
        let mut rng = rand::thread_rng();
        let state: [u8; 16] = rng.gen();
        let state_id = base64_url(&state);
        let now = Instant::now();
        // 期限切れ分を掃除
        self.pending
            .write()
            .unwrap()
            .retain(|_, v| v.expires_at > now);
        self.pending.write().unwrap().insert(
            state_id.clone(),
            PendingState {
                code_challenge,
                expires_at: now + Duration::from_secs(600),
            },
        );
        state_id
    }

    pub fn take(&self, state_id: &str) -> Option<String> {
        let now = Instant::now();
        let mut pending = self.pending.write().unwrap();
        pending.retain(|_, v| v.expires_at > now);
        pending.remove(state_id).map(|v| v.code_challenge)
    }
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(bytes)
}

/// `GET /auth/github/start`
///
/// `redirect` クエリパラメータ（任意）を保存し、GitHub の認可 URL へ 302 する。
pub async fn start(
    State(state): State<AppState>,
    Query(params): Query<StartParams>,
) -> Result<Response, ApiError> {
    let config = &state.config;
    config
        .github_oauth_ready()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let code_verifier = random_verifier();
    let code_challenge = GitHubClient::pkce_challenge(&code_verifier);
    let state_id = state.state_store.save(code_challenge);
    // code_verifier は callback まで保持（最小実装ではインメモリ）。
    state
        .verifier_store
        .write()
        .unwrap()
        .insert(state_id.clone(), code_verifier);

    let redirect = params.redirect.unwrap_or_else(|| "/".to_string());
    state
        .redirect_store
        .write()
        .unwrap()
        .insert(state_id.clone(), redirect);

    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope=read:user&state={}&code_challenge={}&code_challenge_method=S256",
        urlencoding(&config.github_oauth_client_id),
        urlencoding(&config.github_oauth_redirect_uri),
        urlencoding(&state_id),
        urlencoding(&code_challenge),
    );
    Ok(Redirect::to(&url).into_response())
}

#[derive(Debug, Deserialize)]
pub struct StartParams {
    redirect: Option<String>,
}

/// `GET /auth/github/callback`
///
/// `code`／`state` を検証し、トークン交換→セッション作成→クライアントコールバックへ 302。
pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Result<Response, ApiError> {
    let code_challenge = state
        .state_store
        .take(&params.state)
        .ok_or_else(|| ApiError::ValidationFailed("state が無効または期限切れです".into()))?;
    let code_verifier = state
        .verifier_store
        .write()
        .unwrap()
        .remove(&params.state)
        .ok_or_else(|| ApiError::ValidationFailed("state が無効です".into()))?;
    if code_challenge != GitHubClient::pkce_challenge(&code_verifier) {
        return Err(ApiError::VerificationFailed(
            "PKCE の検証に失敗しました".into(),
        ));
    }

    let identity = state
        .github_adapter
        .exchange_code_for_identity(&params.code, &code_verifier)
        .await
        .map_err(|e| map_port_error(e))?;

    let session = state.sessions.create(&identity.id);
    let token_pair = TokenPair {
        access_token: session.session_id.clone(),
        refresh_token: session.refresh_token.clone(),
        expires_in: session.expires_in,
    };
    let redirect = state
        .redirect_store
        .write()
        .unwrap()
        .remove(&params.state)
        .unwrap_or_else(|| "/".to_string());
    let url = format!("{redirect}?hub_token={}", token_pair.access_token);
    Ok(Redirect::to(&url).into_response())
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    code: String,
    state: String,
}

/// `POST /auth/refresh`
///
/// Bearer リフレッシュトークンを検証し、ローテーションしたトークン対を返す。
pub async fn refresh(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<TokenPair>, ApiError> {
    let token = bearer_token(&headers)?;
    let session = state.sessions.rotate(token).map_err(|e| match e {
        crate::session::SessionError::Expired => ApiError::SessionExpired(
            "更新トークンが無効または期限切れです".into(),
        ),
        crate::session::SessionError::Reused => ApiError::RefreshTokenReused(
            "更新トークンの再利用が検知されたためセッションを失効しました".into(),
        ),
    })?;
    Ok(Json(TokenPair {
        access_token: session.session_id,
        refresh_token: session.refresh_token,
        expires_in: session.expires_in,
    }))
}

/// `POST /auth/logout`
pub async fn logout(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, ApiError> {
    let session_id = bearer_token(&headers)?;
    let session = state
        .sessions
        .verify_access(&session_id)
        .ok_or_else(|| ApiError::AuthenticationRequired("セッションが無効です".into()))?;
    state.sessions.revoke(&session.session_id);
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /auth/account`
///
/// アカウント削除（当該利用者の全セッションを失効）。
pub async fn account(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, ApiError> {
    let session_id = bearer_token(&headers)?;
    let session = state
        .sessions
        .verify_access(&session_id)
        .ok_or_else(|| ApiError::AuthenticationRequired("セッションが無効です".into()))?;
    state.sessions.revoke_all_for_user(&session.user_id);
    Ok(StatusCode::NO_CONTENT)
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Result<String, ApiError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::AuthenticationRequired("認証ヘッダーがありません".into()))?;
    value
        .strip_prefix("Bearer ")
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .ok_or_else(|| ApiError::AuthenticationRequired("Bearer トークンが无效です".into()))
}

fn map_port_error(e: crate::ports::PortError) -> ApiError {
    match e {
        crate::ports::PortError::NotFound => ApiError::NotFound("対象が見つかりません".into()),
        crate::ports::PortError::Conflict => ApiError::Conflict("競合が発生しました".into()),
        crate::ports::PortError::PermissionDenied => {
            ApiError::PermissionDenied("権限が不足しています".into())
        }
        crate::ports::PortError::RateLimited => {
            ApiError::RateLimited("レートリミットに達しました".into())
        }
        crate::ports::PortError::Unavailable => {
            ApiError::UpstreamUnavailable("外部基盤が利用できません".into())
        }
        crate::ports::PortError::InvalidResponse => {
            ApiError::Internal("外部基盤からの応答が不正です".into())
        }
        crate::ports::PortError::Unknown => ApiError::Internal("不明なエラー".into()),
    }
}

fn random_verifier() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    base64_url(&bytes)
}

/// state 保存用の共有型。
pub type SharedStateStore = Arc<StateStore>;

/// OAuth 設定からクライアントを生成するヘルパー。
pub fn build_client(
    oauth: &OAuthConfig,
    timeout_read: Duration,
    timeout_write: Duration,
) -> GitHubClient {
    GitHubClient::new("https://api.github.com", timeout_read, timeout_write)
}

/// URL クエリ値として安全な文字列へのエンコード（最小実装）。
fn urlencoding(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}
