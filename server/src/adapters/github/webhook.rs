//! GitHub Webhook ペイロードの受信解析（アダプター層）。
//!
//! GitHub の概念（イベント名・`repository.full_name` 等）はこのモジュールに
//! 閉じ込め、hub 側の `WebhookEvent` へ写す。API 契約・フロントエンドには
//! GitHub の語彙を漏らさない（設計 §7.1）。

use serde_json::Value;

use crate::ports::{PortError, WebhookEvent, WebhookEventKind};

/// `X-GitHub-Event` ヘッダとボディから hub の `WebhookEvent` へ写す。
///
/// - イベント種別は `X-GitHub-Event` ヘッダの `push` を `WebhookEventKind::Push`
///   とし、それ以外は `WebhookEventKind::Other` とする（再生成対象は push）。
/// - 対象リポジトリはボディの `repository.full_name`（`owner/repo`、shell_id 相当）。
/// - ボディが JSON でない、または対象リポジトリが不明な場合は `InvalidResponse`。
pub fn parse_webhook_event(
    event_header: Option<&str>,
    body: &[u8],
) -> Result<WebhookEvent, PortError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| PortError::InvalidResponse)?;

    let kind = match event_header {
        Some("push") => WebhookEventKind::Push,
        _ => WebhookEventKind::Other,
    };

    let repo_full_name = value
        .get("repository")
        .and_then(|r| r.get("full_name"))
        .and_then(|v| v.as_str())
        .ok_or(PortError::InvalidResponse)?
        .to_string();

    Ok(WebhookEvent {
        kind,
        repo_full_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_body(full_name: &str) -> Vec<u8> {
        serde_json::json!({
            "action": "created",
            "repository": { "full_name": full_name, "default_branch": "main" },
            "push_changes": []
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn parses_push_event() {
        let ev = parse_webhook_event(Some("push"), &push_body("octo/demo")).unwrap();
        assert_eq!(ev.kind, WebhookEventKind::Push);
        assert_eq!(ev.repo_full_name, "octo/demo");
    }

    #[test]
    fn non_push_event_is_other() {
        let ev = parse_webhook_event(Some("ping"), &push_body("octo/demo")).unwrap();
        assert_eq!(ev.kind, WebhookEventKind::Other);
        assert_eq!(ev.repo_full_name, "octo/demo");
    }

    #[test]
    fn missing_event_header_is_other() {
        let ev = parse_webhook_event(None, &push_body("octo/demo")).unwrap();
        assert_eq!(ev.kind, WebhookEventKind::Other);
    }

    #[test]
    fn invalid_json_rejected() {
        assert_eq!(
            parse_webhook_event(Some("push"), b"not json"),
            Err(PortError::InvalidResponse)
        );
    }

    #[test]
    fn missing_repository_rejected() {
        let body = serde_json::json!({ "action": "created" }).to_string().into_bytes();
        assert_eq!(
            parse_webhook_event(Some("push"), &body),
            Err(PortError::InvalidResponse)
        );
    }
}
