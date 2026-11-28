//! `ContentPort`（ツリー・ファイル読み取り）の契約テスト。
//!
//! GitHub の contents API はテキストもバイナリも **base64**（60 文字ごとの
//! 改行入り）で返す。hub の契約 `FileContent` は `encoding=utf8` のとき
//! `content`、`encoding=base64` のとき `data` を要求するため、アダプターは
//! 復号して UTF-8 妥当性で振り分ける（判断: `REF:docs/handoff.md` §4 判断 28）。
//! ここでは実 HTTP（ローカルの mock）越しにその振り分けと、内容を壊さない
//! ことを検証する。

use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::client::{GitHubClient, OAuthConfig, Token};
use super::GitHubAdapter;
use crate::ports::{ContentPort, FileEncoding};

/// base64 のファイル応答を返す（GitHub と同じく 60 文字ごとに改行）。
async fn contents(State(bytes): State<Vec<u8>>, req: Request<Body>) -> Response {
    let path = req.uri().path().to_string();
    if path.ends_with("/contents/") || path.ends_with("/contents") {
        return (
            StatusCode::OK,
            axum::Json(serde_json::json!([
                {"path": "contents", "sha": "s1", "type": "dir"},
                {"path": "project.toml", "sha": "s2", "type": "file", "size": 10}
            ])),
        )
            .into_response();
    }
    let encoded = STANDARD.encode(&bytes);
    let wrapped = encoded
        .as_bytes()
        .chunks(60)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "content": wrapped,
            "encoding": "base64",
            "sha": "blob-sha"
        })),
    )
        .into_response()
}

async fn start_mock(bytes: Vec<u8>) -> String {
    let app = Router::new()
        .route("/repos/{owner}/{repo}/contents", get(contents))
        .route("/repos/{owner}/{repo}/contents/{*path}", get(contents))
        .with_state(bytes);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn adapter(base_url: &str) -> GitHubAdapter {
    let client = GitHubClient::new(
        base_url,
        base_url,
        Duration::from_secs(5),
        Duration::from_secs(10),
    );
    let oauth = OAuthConfig {
        client_id: "cid".into(),
        client_secret: "secret".into(),
        redirect_uri: "http://localhost/cb".into(),
    };
    let adapter = GitHubAdapter::new(
        client,
        oauth,
        std::sync::Arc::new(std::sync::RwLock::new(None)),
    );
    adapter.set_token(Token::new("test-token".into()));
    adapter
}

#[tokio::test]
async fn read_file_returns_utf8_text_for_text_bodies() {
    let base = start_mock("# 第一章\n\n本文です。\n".as_bytes().to_vec()).await;
    let adapter = adapter(&base);
    let file = adapter
        .read_file("octo/demo", "contents/01.cnov")
        .await
        .unwrap();
    assert_eq!(file.encoding, FileEncoding::Text);
    assert_eq!(file.content, "# 第一章\n\n本文です。\n");
    assert_eq!(file.etag, "blob-sha");
}

#[tokio::test]
async fn read_file_keeps_binary_bodies_as_base64() {
    // PNG のシグネチャ（UTF-8 として不正なバイト列）。
    let bytes = vec![0x89u8, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0x00];
    let base = start_mock(bytes.clone()).await;
    let adapter = adapter(&base);
    let file = adapter
        .read_file("octo/demo", "images/cover.png")
        .await
        .unwrap();
    assert_eq!(file.encoding, FileEncoding::Base64);
    // 内容が壊れていない（再エンコードした base64 が元のバイト列に戻る）。
    let compact: String = file
        .content
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(STANDARD.decode(compact).unwrap(), bytes);
}

#[tokio::test]
async fn list_tree_maps_directory_and_file_entries() {
    let base = start_mock(b"x".to_vec()).await;
    let adapter = adapter(&base);
    let entries = adapter.list_tree("octo/demo", None).await.unwrap();
    assert_eq!(entries.len(), 2);
    let dir = entries.iter().find(|e| e.path == "contents").unwrap();
    assert_eq!(dir.kind, crate::ports::TreeEntryKind::Directory);
    assert_eq!(dir.etag, "s1");
    let file = entries.iter().find(|e| e.path == "project.toml").unwrap();
    assert_eq!(file.kind, crate::ports::TreeEntryKind::File);
    assert_eq!(file.size, Some(10));
}
