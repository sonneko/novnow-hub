//! リリース（版）レコードの hub 管理ストア（契約 `createRelease`）。
//!
//! 判断（`REF:docs/handoff.md` §4 判断 2・28）により、プロジェクト登録簿
//! （`ProjectRegistry`）と同じく DB を導入せず、インメモリで保持する。
//! 保管基盤の語彙（リポジトリ・タグの実体等）は本ストアの内側に留め、
//! API には契約 `Release` の項目（tag・name・notes・createdAt）だけを返す。
//!
//! 同じタグ・同じ内容の再送は既存レコードを返し、冪等に扱う（契約の
//! `Idempotency-Key` は受領するが、再送判定はタグと内容の一致で行う）。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::registry::now_unix;

/// リリース 1 件の hub レコード。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseRecord {
    /// 版の識別子（契約の `tag`。`v1.2.3` 形式）。
    pub tag: String,
    /// 版の名称（表示用）。
    pub name: String,
    /// 版の説明（任意）。
    pub notes: Option<String>,
    /// 作成日時（UNIX 秒）。
    pub created_at: i64,
}

/// 作成に失敗した理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseError {
    /// 同じタグが別の内容（名称または説明の不一致）で既に存在する。
    DuplicateTag,
}

/// プロジェクト別のリリース保管（インメモリ）。
#[derive(Debug, Default)]
pub struct ReleaseStore {
    items: RwLock<HashMap<String, Vec<ReleaseRecord>>>,
}

impl ReleaseStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// リリースを作成する。
    ///
    /// 同じタグ・同じ内容の再送は既存レコードを返す（冪等）。同じタグで
    /// 内容が異なる場合は [`ReleaseError::DuplicateTag`] を返す。
    pub fn create(
        &self,
        project_id: &str,
        tag: &str,
        name: &str,
        notes: Option<&str>,
    ) -> Result<ReleaseRecord, ReleaseError> {
        let mut guard = self.items.write().unwrap();
        let list = guard.entry(project_id.to_string()).or_default();
        if let Some(existing) = list.iter().find(|r| r.tag == tag) {
            if existing.name == name && existing.notes.as_deref() == notes {
                return Ok(existing.clone());
            }
            return Err(ReleaseError::DuplicateTag);
        }
        let record = ReleaseRecord {
            tag: tag.to_string(),
            name: name.to_string(),
            notes: notes.map(str::to_string),
            created_at: now_unix(),
        };
        list.push(record.clone());
        Ok(record)
    }
}

/// 共有ハンドル。
pub type SharedReleaseStore = Arc<ReleaseStore>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_is_idempotent_for_same_payload() {
        let store = ReleaseStore::new();
        let first = store
            .create("prj_1", "v1.0.0", "初版", Some("memo"))
            .unwrap();
        let second = store
            .create("prj_1", "v1.0.0", "初版", Some("memo"))
            .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn create_rejects_conflicting_payload_for_same_tag() {
        let store = ReleaseStore::new();
        store.create("prj_1", "v1.0.0", "初版", None).unwrap();
        let err = store.create("prj_1", "v1.0.0", "別名", None).unwrap_err();
        assert_eq!(err, ReleaseError::DuplicateTag);
    }

    #[test]
    fn tags_are_scoped_per_project() {
        let store = ReleaseStore::new();
        store.create("prj_1", "v1.0.0", "初版", None).unwrap();
        assert!(store.create("prj_2", "v1.0.0", "初版", None).is_ok());
    }
}
