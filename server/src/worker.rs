//! Cloudflare Workers (workerd) 用の wasm-bindgen エントリポイント。
//!
//! ビルド: `cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm`
//!
//! workerd は HTTP リクエストごとにこのモジュールの `fetch` を呼び出す。
//!
//! 現状の適応範囲:
//!   - `GET /api/v1/meta`（契約 `Meta`）を wasm-bindgen + web-sys で応答。
//!   - その他のパスは body に `not_found` を含むレスポンスを返す。
//!   - 静的公開ページは wrangler の `assets`（`web/dist`）が配信する。
//!
//! 未対応 / アカウント側操作待ち（判断: REF:docs/handoff.md §4 判断 26）:
//!   - D1 への実接続（永続化）。ワーカ側の D1 バインディング（`[[d1_databases]]`）の
//!     作成・wrangler.toml への登録・Secrets 設定は Cloudflare アカウント側の操作が必要。
//!     ここではコード側のインターフェース（`ports::D1Port`）のみを整え、実接続は行わない。
//!   - GitHub OAuth / API アクセス（reqwest 依存）は wasm では無効化。認証フローは
//!     アカウント側で Secrets（OAuth Client ID/Secret 等）を設定後、後続で
//!     wasm-bindgen の `web_sys::Window::fetch_with_request` を介した HTTP クライアント
//!     実装を ports に追加する予定。

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use web_sys::{Headers, Request, Response, ResponseInit};

use crate::config::Config;
use web_sys::js_sys::Promise;

/// リクエスト URL からパス部分を抽出する（web-sys の `Url` はステータス設定周りの
  /// バージョン制約により使わず、ここでは文字列操作で抽出する）。
fn extract_path(url: &str) -> String {
    let without_scheme = url.split("://").nth(1).unwrap_or(url);
    let path_and_query = without_scheme.split('/').skip(1).collect::<Vec<_>>();
    if path_and_query.is_empty() {
        return "/".to_string();
    }
    let path = path_and_query.join("/");
    let path = path.split('?').next().unwrap_or(&path);
    let path = path.split('#').next().unwrap_or(path);
    format!("/{path}")
}

/// Cloudflare Workers (workerd) 用の fetch エントリポイント。
///
/// workerd はこの関数を HTTP リクエストごとに呼び出す。`wasm-bindgen` が
/// `async fn` を直接エクスポートできないため、`wasm-bindgen-futures` の
/// `future_to_promise` で `js_sys::Promise` を返す。
#[wasm_bindgen]
pub fn fetch(req: Request) -> Promise {
    let fut = async move { handle(req).await };
    future_to_promise(fut)
}

/// リクエストをパスに基づいて振り分け、レスポンスを生成する。
async fn handle(req: Request) -> Result<JsValue, JsValue> {
    let url = req.url(); // web-sys 0.3 の `Request::url()` は `String` を返す
    let path = extract_path(&url);

    let response = match path.as_str() {
        "/api/v1/meta" => meta_response().await?,
        _ => not_found_response(&path)?,
    };

    // web_sys::Response は wasm-bindgen の WebSys 型であり JsValue へ変換可能。
    Ok(response.into())
}

/// `GET /api/v1/meta` のレスポンス（契約 `Meta` に準拠）。
///
/// `chrono` と `Config::from_env()` は wasm でも動作するため、
/// Workers 上の環境変数（`vars` / Secrets）から契約版などを読める。
async fn meta_response() -> Result<Response, JsValue> {
    let config = Config::from_env();
    let body = serde_json::json!({
        "contractVersion": config.contract_version,
        "serverTime": chrono::Utc::now().to_rfc3339(),
        "minimumClientVersion": config.minimum_client_version,
    })
    .to_string();

    let headers = Headers::new()?;
    headers.set("content-type", "application/json")?;

    let init = ResponseInit::new();
    init.set_status(200);
    init.set_headers(&headers);

    let response = Response::new_with_opt_str_and_init(Some(&body), &init)?;
    Ok(response)
}

/// not_found レスポンス（ステータス 404 を返す）。
fn not_found_response(path: &str) -> Result<Response, JsValue> {
    let body = serde_json::json!({ "error": "not_found", "path": path }).to_string();

    let headers = Headers::new()?;
    headers.set("content-type", "application/json")?;

    let init = ResponseInit::new();
    init.set_status(404);
    init.set_headers(&headers);

    let response = Response::new_with_opt_str_and_init(Some(&body), &init)?;
    Ok(response)
}

/// D1 データベースへのアクセスポート（コード側の準備）。
///
/// 実接続はアカウント側の D1 バインディング設定待ち（判断 REF:docs/handoff.md §4 判断 26）。
/// ここではコード側のインターフェース（`ports::D1Port`）のみを整え、実接続は行わない。
#[allow(dead_code)]
pub async fn d1_placeholder() -> Result<(), JsValue> {
    // D1 バインディングは wrangler.toml の [[d1_databases]] でワーカに注入される。
    // wasm-bindgen で JS 側の `env.DB` にアクセスするには、workerd の wasm エクスポート
    // または JS の glue コードが必要であり、本環境（vendored crates のみ、workers-rs 未導入）
    // では実装できない。アカウント側で D1 データベースを作成・バインディングし、
    // wrangler.toml / Secrets を設定した後、本関数を実装に置き換える。
    Err(JsValue::from_str(
        "D1 binding is not wired; account-side D1 setup is required (judgment 26)",
    ))
}
