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
    }
}

/// テスト用の AppState を組み立てる。
pub fn test_state() -> AppState {
    crate::build_app_state(test_config())
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
