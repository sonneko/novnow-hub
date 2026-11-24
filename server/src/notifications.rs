//! 通知の hub 管理ストア（設計 §6）。
//!
//! 通知は hub 管理ストアにのみ保存し、外部基盤には送らない（設計 §6.1）。
//! ドメインイベントから `emit` でファンアウトし、通知センターが
//! `list_notifications`・`mark_read` で操作する。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use crate::ports::{Notification, NotificationKind, NotificationPort, PortError};

/// 共有される通知ストア。
pub type SharedNotificationStore = Arc<NotificationStore>;

/// インメモリの通知ストア。利用者ごとに通知を保持する。
#[derive(Default)]
pub struct NotificationStore {
    /// user_id → 通知一覧（新しい順）。
    by_user: RwLock<HashMap<String, Vec<Notification>>>,
    id_counter: AtomicU64,
}

impl NotificationStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_id(&self) -> String {
        let n = self.id_counter.fetch_add(1, Ordering::Relaxed);
        format!("notif-{n}")
    }

    /// 指定利用者の未読通知件数（`X-Unread-Count` ヘッダ用）。
    pub fn unread_count(&self, user_id: &str) -> usize {
        self.by_user
            .read()
            .map(|store| {
                store
                    .get(user_id)
                    .map(|list| list.iter().filter(|n| !n.read).count())
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }
}

impl NotificationPort for NotificationStore {
    async fn list_notifications(
        &self,
        user_id: &str,
        kind: Option<NotificationKind>,
        unread_only: bool,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<Notification>, Option<String>), PortError> {
        let store = self.by_user.read().map_err(|_| PortError::Unavailable)?;
        let mut items: Vec<Notification> = store
            .get(user_id)
            .map(|list| {
                list.iter()
                    .filter(|n| kind.is_none_or(|k| n.kind == k))
                    .filter(|n| !unread_only || !n.read)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        items.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });

        let offset = cursor.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);
        let offset = offset.min(items.len());
        let total = items.len();
        let page: Vec<Notification> = items.into_iter().skip(offset).take(limit).collect();
        let next_offset = offset + page.len();
        let next_cursor = if next_offset < total {
            Some(next_offset.to_string())
        } else {
            None
        };
        Ok((page, next_cursor))
    }

    async fn mark_read(
        &self,
        user_id: &str,
        ids: &[String],
        all: bool,
    ) -> Result<(), PortError> {
        let mut store = self.by_user.write().map_err(|_| PortError::Unavailable)?;
        let Some(list) = store.get_mut(user_id) else {
            return Ok(());
        };
        if all {
            for notification in list {
                notification.read = true;
            }
            return Ok(());
        }
        for notification in list {
            if ids.contains(&notification.id) {
                notification.read = true;
            }
        }
        Ok(())
    }

    async fn emit(
        &self,
        user_id: &str,
        kind: NotificationKind,
        project_id: Option<&str>,
        project_slug: Option<&str>,
        title: &str,
        body: &str,
    ) -> Result<(), PortError> {
        let now = chrono::Utc::now().timestamp();
        let notification = Notification {
            id: self.next_id(),
            kind,
            project_id: project_id.map(|s| s.to_string()),
            project_slug: project_slug.map(|s| s.to_string()),
            title: title.to_string(),
            body: body.to_string(),
            read: false,
            created_at: now,
        };
        let mut store = self.by_user.write().map_err(|_| PortError::Unavailable)?;
        store.entry(user_id.to_string()).or_default().push(notification);
        Ok(())
    }
}
