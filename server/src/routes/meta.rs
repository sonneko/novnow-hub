//! `GET /meta` ルーター。
//!
//! 契約 `REF:api/openapi.yaml` の `GET /meta` に準拠する。

use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

use crate::app_state::AppState;

/// 応答スキーマ（契約の `Meta` に準拠）。
#[derive(Debug, Serialize)]
pub struct Meta {
    #[serde(rename = "contractVersion")]
    pub contract_version: String,
    #[serde(rename = "serverTime")]
    pub server_time: String,
    #[serde(rename = "minimumClientVersion", skip_serializing_if = "Option::is_none")]
    pub minimum_client_version: Option<String>,
}

/// `GET /meta`
pub async fn get_meta(State(state): State<AppState>) -> (HeaderMap, Json<Meta>) {
    let headers = HeaderMap::new();
    let meta = Meta {
        contract_version: state.config.contract_version.clone(),
        server_time: chrono::Utc::now().to_rfc3339(),
        minimum_client_version: None,
    };
    (headers, Json(meta))
}
