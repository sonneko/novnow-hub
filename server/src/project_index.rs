//! プロジェクト一覧・検索の軽量インデックス。
//!
//! 仕様 `REF:docs/specifications/m3.md` §4.1・§4.2、設計
//! `REF:docs/designs/m3.md` §4.1・§4.2 に準拠する。
//!
//! M3 では DB を導入せず、インメモリ登録簿（`ProjectRegistry`）のスナップショット
//! を走査して一覧・検索を実現する「軽量インデックス」とする（判断
//! `REF:docs/handoff.md` §4 判断 2 の延長）。カーソルページネーションは
//! 明示的な「次へ」に対応し、総件数は返さない（設計 §4.2）。
//!
//! 本モジュールは純粋関数（HTTP 非依存）とし、登録簿の実装差し替えやテストを
//! 容易にする。GitHub の概念はここに漏れない（メタのみを扱う）。

use crate::ports::ProjectVisibility;
use crate::registry::ProjectRecord;

/// 一覧の並び替えキー（契約 `sort` の列挙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSort {
    /// 更新順（既定）。
    Updated,
    /// 新着順。
    Created,
    /// 派生数順。
    Derivatives,
}

impl ListSort {
    /// 契約の列挙値から解釈する（不正は `None`）。
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "updated" => Some(ListSort::Updated),
            "created" => Some(ListSort::Created),
            "derivatives" => Some(ListSort::Derivatives),
            _ => None,
        }
    }

    /// カーソルに埋め込む並び順タグ。
    pub fn tag(self) -> &'static str {
        match self {
            ListSort::Updated => "updated",
            ListSort::Created => "created",
            ListSort::Derivatives => "derivatives",
        }
    }
}

/// 検索の並び替えキー（契約 `sort` の列挙）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSort {
    /// 関連度順（既定）。
    Relevance,
    /// 更新順。
    Updated,
}

impl SearchSort {
    /// 契約の列挙値から解釈する（不正は `None`）。
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "relevance" => Some(SearchSort::Relevance),
            "updated" => Some(SearchSort::Updated),
            _ => None,
        }
    }

    /// カーソルに埋め込む並び順タグ。
    pub fn tag(self) -> &'static str {
        match self {
            SearchSort::Relevance => "relevance",
            SearchSort::Updated => "updated",
        }
    }
}

/// ページング結果（総件数は返さない＝設計 §4.2）。
#[derive(Debug, Clone)]
pub struct Page {
    pub items: Vec<ProjectRecord>,
    pub next_cursor: Option<String>,
}

/// 公開範囲が公開のもののみを残す（一覧・検索は公開作品のみ）。
fn public_only(records: &[ProjectRecord]) -> Vec<ProjectRecord> {
    records
        .iter()
        .filter(|r| r.visibility == ProjectVisibility::Public)
        .cloned()
        .collect()
}

/// 一覧のソートキー。
///
/// 降順で並べる。`(主キー, 副キー, プロジェクトID)` のタプルで全順序を構成し、
/// カーソルによる「次のページ」の判定に使う。
fn list_key(record: &ProjectRecord, sort: ListSort) -> (i64, i64, String) {
    match sort {
        ListSort::Updated => (record.updated_at, 0, record.project_id.clone()),
        ListSort::Created => (record.created_at, 0, record.project_id.clone()),
        // 派生数は M3 では常に 0。主キーが同一のため更新日時で副排序する。
        ListSort::Derivatives => (
            0,
            record.updated_at,
            record.project_id.clone(),
        ),
    }
}

/// 検索のソートキー（関連度順は `(関連度, 更新日時, ID)`）。
fn search_key(record: &ProjectRecord, sort: SearchSort, q: &str) -> (i64, i64, String) {
    match sort {
        SearchSort::Updated => (record.updated_at, 0, record.project_id.clone()),
        SearchSort::Relevance => (relevance(record, q), record.updated_at, record.project_id.clone()),
    }
}

/// キーワードに対する関連度（軽量。タイトル＞ハンドル＞説明・タグ）。
fn relevance(record: &ProjectRecord, q: &str) -> i64 {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return 0;
    }
    let mut score = 0;
    if record.title.to_lowercase().contains(&q) {
        score += 2;
    }
    if record.owner_handle.to_lowercase().contains(&q) {
        score += 1;
    }
    if let Some(d) = &record.description {
        if d.to_lowercase().contains(&q) {
            score += 1;
        }
    }
    if record.tags.iter().any(|t| t.to_lowercase().contains(&q)) {
        score += 1;
    }
    score
}

/// キーワード一致（関連度 > 0 なら対象）。
fn matches_keyword(record: &ProjectRecord, q: &str) -> bool {
    relevance(record, q) > 0
}

/// タグ絞り込み（部分一致・大文字小文字を問わない。フロントと同一の判定）。
fn matches_tag(record: &ProjectRecord, tag: &str) -> bool {
    let tag = tag.trim().to_lowercase();
    if tag.is_empty() {
        return true;
    }
    record
        .tags
        .iter()
        .any(|t| t.to_lowercase().contains(&tag))
}

/// カーソルをエンコードする（不透明文字列。並び順タグ＋最終項目の ID）。
fn encode_cursor(sort_tag: &str, project_id: &str) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(format!("{sort_tag}|{project_id}"))
}

/// カーソルをデコードし、並び順が一致することを検証する。
///
/// 不一致・不正・期限切れ（項目が既に無い）は `Err` を返す（呼び出し側が 400 に写像）。
fn decode_cursor(cursor: &str, expected_sort_tag: &str) -> Result<String, String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let raw = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|e| format!("カーソルが不正です: {e}"))?;
    let text = String::from_utf8(raw).map_err(|e| format!("カーソルが不正です: {e}"))?;
    let mut parts = text.splitn(2, '|');
    let sort_tag = parts.next().unwrap_or("");
    let project_id = parts.next().unwrap_or("");
    if sort_tag != expected_sort_tag {
        return Err("カーソルの並び順がリクエストと一致しません".into());
    }
    if project_id.is_empty() {
        return Err("カーソルが不正です".into());
    }
    Ok(project_id.to_string())
}

/// ソート済み配列をカーソル位置以降でページングする。
///
/// `sorted` は既に降順でソート済み。`cursor` が `Some` ならその項目の直後から、
/// `None` なら先頭から `limit` 件を取る。`next_cursor` は次ページが無ければ `None`。
fn paginate(
    sorted: &[ProjectRecord],
    sort_tag: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<Page, String> {
    let start = match cursor {
        Some(c) => {
            let pid = decode_cursor(c, sort_tag)?;
            match sorted.iter().position(|r| r.project_id == pid) {
                Some(idx) => idx + 1,
                None => return Err("カーソルが期限切れです".into()),
            }
        }
        None => 0,
    };
    let total = sorted.len();
    let end = (start + limit).min(total);
    let items: Vec<ProjectRecord> = if start < total {
        sorted[start..end].to_vec()
    } else {
        Vec::new()
    };
    let next_cursor = if end < total {
        items.last().map(|r| encode_cursor(sort_tag, &r.project_id))
    } else {
        None
    };
    Ok(Page {
        items,
        next_cursor,
    })
}

/// 一覧（設計 §4.1）。公開作品を並べ替え＋カーソルページネーション。
pub fn list(
    records: &[ProjectRecord],
    sort: ListSort,
    cursor: Option<&str>,
    limit: usize,
) -> Result<Page, String> {
    let mut public = public_only(records);
    // 降順ソート（更新日時＋ID の複合。全順序を保証）。
    public.sort_by(|a, b| {
        list_key(b, sort).cmp(&list_key(a, sort))
    });
    paginate(&public, sort.tag(), cursor, limit)
}

/// 検索（設計 §4.2）。キーワード・タグ・ライセンスで絞り込み、軽量インデックス上で実行。
pub fn search(
    records: &[ProjectRecord],
    q: &str,
    license: Option<&str>,
    tag: Option<&str>,
    sort: SearchSort,
    cursor: Option<&str>,
    limit: usize,
) -> Result<Page, String> {
    let mut matched: Vec<ProjectRecord> = public_only(records)
        .into_iter()
        .filter(|r| matches_keyword(r, q))
        .filter(|r| license.map(|l| r.license == l).unwrap_or(true))
        .filter(|r| matches_tag(r, tag.unwrap_or("")))
        .collect();
    matched.sort_by(|a, b| {
        search_key(b, sort, q).cmp(&search_key(a, sort, q))
    });
    paginate(&matched, sort.tag(), cursor, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::ProjectVisibility;
    use crate::registry::{now_unix, ProjectRecord};

    fn rec(id: &str, title: &str, license: &str, tag: &[&str], updated: i64) -> ProjectRecord {
        ProjectRecord {
            project_id: id.to_string(),
            title: title.to_string(),
            description: Some(format!("desc {title}")),
            tags: tag.iter().map(|t| t.to_string()).collect(),
            owner_handle: format!("owner-{id}"),
            license: license.to_string(),
            derivative_policy: "allowed".to_string(),
            visibility: ProjectVisibility::Public,
            shell_id: format!("shell-{id}"),
            default_branch: "main".to_string(),
            etag: format!("etag-{id}"),
            created_at: updated,
            updated_at: updated,
        }
    }

    fn sample_set() -> Vec<ProjectRecord> {
        // 更新日時が異なる 3 件。
        vec![
            rec("a", "Moonlit Garden", "cc-by", &["fantasy"], 100),
            rec("b", "Star Voyage", "cc0", &["scifi"], 200),
            rec("c", "Deep Forest", "cc-by-sa", &["nature"], 300),
        ]
    }

    #[test]
    fn list_sorted_by_updated_desc() {
        let page = list(&sample_set(), ListSort::Updated, None, 10).unwrap();
        let ids: Vec<&str> = page.items.iter().map(|r| r.project_id.as_str()).collect();
        // 更新日時降順 → c, b, a。
        assert_eq!(ids, vec!["c", "b", "a"]);
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn list_sorted_by_created_desc() {
        let page = list(&sample_set(), ListSort::Created, None, 10).unwrap();
        let ids: Vec<&str> = page.items.iter().map(|r| r.project_id.as_str()).collect();
        // 作成日時降順（created == updated のため）→ c, b, a。
        assert_eq!(ids, vec!["c", "b", "a"]);
    }

    #[test]
    fn list_cursor_pagination() {
        // 1 件ずつ返す。1 ページ目は c、cursor で 2 ページ目は b。
        let p1 = list(&sample_set(), ListSort::Updated, None, 1).unwrap();
        assert_eq!(p1.items.len(), 1);
        assert_eq!(p1.items[0].project_id, "c");
        let c1 = p1.next_cursor.unwrap();

        let p2 = list(&sample_set(), ListSort::Updated, Some(&c1), 1).unwrap();
        assert_eq!(p2.items[0].project_id, "b");
        let c2 = p2.next_cursor.unwrap();

        let p3 = list(&sample_set(), ListSort::Updated, Some(&c2), 1).unwrap();
        assert_eq!(p3.items[0].project_id, "a");
        assert!(p3.next_cursor.is_none());
    }

    #[test]
    fn list_private_excluded() {
        let mut set = sample_set();
        set.push(rec("priv", "Secret", "cc0", &[], 999));
        set.last_mut().unwrap().visibility = ProjectVisibility::Private;
        let page = list(&set, ListSort::Updated, None, 10).unwrap();
        assert!(!page.items.iter().any(|r| r.project_id == "priv"));
        assert_eq!(page.items.len(), 3);
    }

    #[test]
    fn list_invalid_sort_rejected() {
        assert!(ListSort::parse("bogus").is_none());
        assert_eq!(ListSort::parse("updated"), Some(ListSort::Updated));
    }

    #[test]
    fn list_stale_cursor_rejected() {
        // 存在しない項目のカーソルは期限切れ。
        let bad = encode_cursor("updated", "nope");
        let err = list(&sample_set(), ListSort::Updated, Some(&bad), 10).unwrap_err();
        assert!(err.contains("期限切れ"));
    }

    #[test]
    fn search_by_keyword_title() {
        let page = search(&sample_set(), "moonlit", None, None, SearchSort::Relevance, None, 10)
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].project_id, "a");
    }

    #[test]
    fn search_by_owner_handle() {
        let page = search(&sample_set(), "owner-b", None, None, SearchSort::Relevance, None, 10)
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].project_id, "b");
    }

    #[test]
    fn search_by_license_filter() {
        let page = search(&sample_set(), "garden", Some("cc-by"), None, SearchSort::Relevance, None, 10)
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].project_id, "a");
    }

    #[test]
    fn search_by_tag_filter() {
        let page = search(&sample_set(), "garden", None, Some("fantasy"), SearchSort::Relevance, None, 10)
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].project_id, "a");
        // 不一致タグは空。
        let empty = search(&sample_set(), "garden", None, Some("nomatch"), SearchSort::Relevance, None, 10)
            .unwrap();
        assert!(empty.items.is_empty());
    }

    #[test]
    fn search_no_match_is_empty() {
        let page = search(&sample_set(), "zzz", None, None, SearchSort::Relevance, None, 10).unwrap();
        assert!(page.items.is_empty());
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn search_cursor_pagination() {
        let set = sample_set();
        // キーワード "e" は全件に含まれる（garden/voyage/forest の説明）。
        let p1 = search(&set, "e", None, None, SearchSort::Updated, None, 1).unwrap();
        assert_eq!(p1.items.len(), 1);
        assert!(p1.next_cursor.is_some());
        let c = p1.next_cursor.unwrap();
        let p2 = search(&set, "e", None, None, SearchSort::Updated, Some(&c), 1).unwrap();
        assert_eq!(p2.items.len(), 1);
        assert_ne!(p1.items[0].project_id, p2.items[0].project_id);
    }

    #[test]
    fn cursor_sort_mismatch_rejected() {
        // updated のカーソルを relevance で使うと不一致。
        let c = encode_cursor("updated", "c");
        let err = search(&sample_set(), "e", None, None, SearchSort::Relevance, Some(&c), 10)
            .unwrap_err();
        assert!(err.contains("一致"));
    }

    #[test]
    fn relevance_prefers_title_over_description() {
        // タイトル一致の関連度が説明一致より高い。
        let a = rec("a", "Moonlit Garden", "cc-by", &[], 100);
        let b = rec("b", "Other", "cc-by", &[], 100);
        assert!(relevance(&a, "garden") > relevance(&b, "garden"));
        assert!(relevance(&b, "other") > 0);
        assert_eq!(relevance(&b, "zzz"), 0);
    }

    #[test]
    fn now_unix_is_positive() {
        assert!(now_unix() > 0);
    }
}
