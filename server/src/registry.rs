//! インメモリのプロジェクト登録簿（ProjectRegistry）。
//!
//! 判断（`REF:docs/handoff.md` §4 判断 2）により、M3 thin slice では DB を
//! 導入せず、プロジェクトメタをインメモリで保持する。読み取り
//! （`GET /projects/{projectId}`・`/reading`）はポート経由で保管単位から
//! コンテンツを読む。後続で DB 導入時に本登録簿の実装を差し替えのみで移行
//! できる（ポート経由で隔離）。
//!
//! zip は検証後に破棄し、恒久保存しない（仕様 §6.1）。本登録簿はメタのみを
//! 保持し、zip 内容は保持しない。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::ports::ProjectVisibility;

/// 登録簿に保持するプロジェクトメタ。
#[derive(Debug, Clone)]
pub struct ProjectRecord {
    /// プロジェクト識別子（hub 側の不透明文字列）。
    pub project_id: String,
    /// 作品タイトル。
    pub title: String,
    /// 説明（任意）。
    pub description: Option<String>,
    /// タグ（任意。検索・絞り込み用の軽量インデックス）。
    ///
    /// 公開フローの multipart フォームにはタグ項目が無いため、M3 では空配列で
    /// 登録する。後続でタグ入力が追加された際に本フィールドを populating する。
    pub tags: Vec<String>,
    /// 作者ハンドル。
    pub owner_handle: String,
    /// ライセンス（`cc0`・`cc-by` 等。契約の列挙値）。
    pub license: String,
    /// 公開範囲。
    pub visibility: ProjectVisibility,
    /// 保管単位の識別子（ポート経由でコンテンツを読むキー）。
    pub shell_id: String,
    /// 既定分岐名。
    pub default_branch: String,
    /// 版識別子（ETag）。
    pub etag: String,
    /// 作成日時（UNIX 秒）。
    pub created_at: i64,
    /// 更新日時（UNIX 秒）。
    pub updated_at: i64,
}

/// インメモリのプロジェクト登録簿。
#[derive(Debug, Default)]
pub struct ProjectRegistry {
    records: RwLock<HashMap<String, ProjectRecord>>,
}

impl ProjectRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// プロジェクトを登録する。
    pub fn register(&self, record: ProjectRecord) {
        self.records.write().unwrap().insert(record.project_id.clone(), record);
    }

    /// プロジェクトを参照する（不存在は `None`）。
    pub fn get(&self, project_id: &str) -> Option<ProjectRecord> {
        self.records.read().unwrap().get(project_id).cloned()
    }

    /// 保管単位の識別子からプロジェクトを参照する。
    pub fn get_by_shell(&self, shell_id: &str) -> Option<ProjectRecord> {
        self.records
            .read()
            .unwrap()
            .values()
            .find(|r| r.shell_id == shell_id)
            .cloned()
    }

    /// 登録簿の全件をスナップショットとして返す（軽量インデックス用）。
    ///
    /// インメモリ登録簿のため、一覧・検索は本コレクションを走査して実現する。
    /// 後続で DB 導入時は本実装を差し替えのみで移行できる。
    pub fn all(&self) -> Vec<ProjectRecord> {
        self.records.read().unwrap().values().cloned().collect()
    }
}

/// 共有ハンドル。
pub type SharedProjectRegistry = Arc<ProjectRegistry>;

/// 現在の UNIX 秒を返す。
pub fn now_unix() -> i64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// プロジェクト識別子（ULID 風の不透明文字列）を生成する。
///
/// 設計 §2.2 の「ULID 互換の不透明文字列」に準じ、時間成分＋乱数の
/// Base62 風文字列とする。
pub fn new_project_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    // 時間成分（10 桁の 36 進数相当）＋ 乱数成分。
    let mut id = String::with_capacity(26);
    let mut t = now_ms;
    for _ in 0..10 {
        id.push(char::from_digit((t % 36) as u32, 36).unwrap_or('0'));
        t /= 36;
    }
    for _ in 0..16 {
        id.push(char::from_digit(rng.gen_range(0..36), 36).unwrap_or('0'));
    }
    id
}

/// プロジェクト識別子から slug を生成する（URL・保管単位名の用途）。
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in title.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            // 非英数はダッシュに置換（連続は単一化）。
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "project".to_string()
    } else {
        trimmed
    }
}

/// 保管単位名（GitHub のリポジトリ名相当）を生成する。
///
/// 同一利用者の重复を避けるため、`{handle}-{slug}-{短縮}` の形とする。
pub fn shell_name(handle: &str, slug: &str) -> String {
    let base = format!("{handle}-{slug}");
    format!("{}-{}", base, short_random())
}

/// 短い乱数接尾辞。
fn short_random() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut s = String::with_capacity(6);
    for _ in 0..6 {
        s.push(char::from_digit(rng.gen_range(0..36), 36).unwrap_or('0'));
    }
    s
}

/// 既定分岐名（保管単位の既定分岐）。
pub const DEFAULT_BRANCH: &str = "main";

/// 公開範囲を文字列に写像する。
pub fn visibility_str(v: ProjectVisibility) -> &'static str {
    match v {
        ProjectVisibility::Public => "public",
        ProjectVisibility::Private => "private",
        ProjectVisibility::Archived => "archived",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(project_id: &str, shell: &str) -> ProjectRecord {
        ProjectRecord {
            project_id: project_id.to_string(),
            title: "t".to_string(),
            description: None,
            tags: Vec::new(),
            owner_handle: "sora".to_string(),
            license: "cc-by".to_string(),
            visibility: ProjectVisibility::Public,
            shell_id: shell.to_string(),
            default_branch: DEFAULT_BRANCH.to_string(),
            etag: "etag".to_string(),
            created_at: now_unix(),
            updated_at: now_unix(),
        }
    }

    #[test]
    fn register_and_get() {
        let reg = ProjectRegistry::new();
        reg.register(sample("prj_1", "sora/repo1"));
        assert_eq!(reg.get("prj_1").unwrap().title, "t");
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn get_by_shell() {
        let reg = ProjectRegistry::new();
        reg.register(sample("prj_1", "sora/repo1"));
        assert_eq!(reg.get_by_shell("sora/repo1").unwrap().project_id, "prj_1");
        assert!(reg.get_by_shell("nope").is_none());
    }

    #[test]
    fn new_project_id_is_opaque_and_unique() {
        let a = new_project_id();
        let b = new_project_id();
        assert!(!a.is_empty());
        assert_ne!(a, b);
    }

    #[test]
    fn slugify_normalizes() {
        // 英数を含まない入力はフォールバックの `project` になる。
        assert_eq!(slugify("月明かり の 庭"), "project");
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("  --Multiple--Dash--  "), "multiple-dash");
    }

    #[test]
    fn shell_name_includes_handle_and_slug() {
        let name = shell_name("sora", "moonlit-garden");
        assert!(name.starts_with("sora-moonlit-garden-"));
    }

    #[test]
    fn visibility_str_maps() {
        assert_eq!(visibility_str(ProjectVisibility::Public), "public");
        assert_eq!(visibility_str(ProjectVisibility::Private), "private");
        assert_eq!(visibility_str(ProjectVisibility::Archived), "archived");
    }
}
