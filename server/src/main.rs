//! novnow-hub バックエンドのエントリポイント（Rust + Axum）。
//!
//! API 契約 `REF:api/openapi.yaml` にのみ準拠する。契約にない独自拡張はしない。
//! バックエンドの配置箇所は仕様・設計に未定だったため、新規 crate `server/`
//! に配置した（PR 本文に記録する）。
//!
//! 実行モード:
//!   - デフォルト feature `native`（ネイティブビルド）：`tokio` + `axum` で HTTP サーバーを起動。
//!   - feature `wasm`（`--target wasm32-unknown-unknown --no-default-features --features wasm --lib`）：
//!     Cloudflare Workers (workerd) 上で動作する `wasm-bindgen` エントリポイント（`src/worker.rs`）。
//!     wasm ビルドではバイナリ（`[[bin]]`）ではなくライブラリ（`[lib]`, `crate-type = ["cdylng"]`）を
//!     出力するため、`main` 関数は定義しない。

/// ネイティブビルド用のエントリポイント（`tokio` + `axum`）。
///
/// wasm ビルド（Cloudflare Workers）では定義されない（`worker.rs` が wasm-bindgen エントリポイント）。
#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use novnow_hub_server::app_state::AppState;
    use novnow_hub_server::build_app_state;
    use novnow_hub_server::build_router;
    use novnow_hub_server::config::Config;
    use tokio::net::TcpListener;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::from_env();
    tracing::info!("contract version: {}", config.contract_version);
    tracing::info!("listening on: {}:{}", config.bind_addr, config.port);

    let state: AppState = build_app_state(config.clone());
    let app = build_router(state);
    let listener = TcpListener::bind((config.bind_addr.as_str(), config.port)).await?;
    tracing::info!("server started");
    axum::serve(listener, app).await?;
    Ok(())
}
