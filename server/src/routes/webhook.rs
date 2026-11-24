//! Webhook 受信の内部エンドポイント（設計 §7.1）。
//!
//! **サーバー内部の受信口**であり、API 契約（`api/openapi.yaml`）には
//! Webhook 用の公開エンドポイントが存在しないため、契約には追加しない。
//! 外部基盤（GitHub）からのプッシュを HMAC-SHA256 署名検証で受信し、
//! 検証後のイベントに応じて公開ページ再生成ジョブを非同期で起票する。

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::app_state::AppState;
use crate::webhook::{receive_webhook, run_job, WebhookRejection};

/// `POST /internal/webhook` — GitHub Webhook を受信する内部エンドポイント。
///
/// 署名検証に失敗したリクエストは拒否する。成功時は再生成ジョブを非同期で
/// 起票し、受信自体は即座に 202 で応じる（閲覧をブロックしない）。
pub async fn receive(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok());
    let event_header = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok());
    let secret = state.config.webhook_secret.as_deref();

    let outcome = match receive_webhook(&state, secret, event_header, signature, &body).await {
        Ok(outcome) => outcome,
        Err(rejection) => return rejection_response(rejection),
    };

    // 再生成ジョブは非同期で実行し、受信応答はブロックしない。
    if let Some(job) = outcome.job {
        let store = state.webhook.clone();
        let rebuild = state.rebuild.clone();
        let job_id = job.job_id.clone();
        let project_id = job.project_id.clone();
        let repo = job.repo_full_name.clone();
        tokio::spawn(async move {
            run_job(store.as_ref(), rebuild.as_ref(), job_id, project_id, repo).await;
        });
        return (
            StatusCode::ACCEPTED,
            Json(json!({
                "jobId": job.job_id,
                "status": job.status.as_str(),
                "projectId": job.project_id,
            })),
        )
            .into_response();
    }

    // 再生成対象外のイベント（非 push 等）。受信自体は成功として応じる。
    (
        StatusCode::OK,
        Json(json!({ "accepted": true, "skipped": true })),
    )
        .into_response()
}

/// 拒否理由をステータスコードとメッセージに写す（内部エンドポイントのため
/// Problem Details を厳密には要求されないが、簡潔な JSON で返す）。
fn rejection_response(rejection: WebhookRejection) -> Response {
    let (status, code) = match rejection {
        WebhookRejection::MissingSignature | WebhookRejection::InvalidSignature => {
            (StatusCode::UNAUTHORIZED, "invalid_signature")
        }
        WebhookRejection::MissingSecret => (StatusCode::INTERNAL_SERVER_ERROR, "missing_secret"),
        WebhookRejection::InvalidPayload => (StatusCode::BAD_REQUEST, "invalid_payload"),
        WebhookRejection::UnknownProject => (StatusCode::NOT_FOUND, "unknown_project"),
    };
    (status, Json(json!({ "error": code }))).into_response()
}
