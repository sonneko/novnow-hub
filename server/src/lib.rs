//! novnow-hub バックエンドのライブラリ本体。
//!
//! エントリポイント（`main`）とライブラリ（テスト用）を分ける。

pub mod adapters;
pub mod app_state;
pub mod config;
pub mod error;
pub mod middleware;
pub mod ports;
pub mod routes;
pub mod session;
