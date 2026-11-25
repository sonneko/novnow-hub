//! 派生作品の hub 管理ストア（設計 §2.2）。
//!
//! 派生元との対応関係（`parent_id` / `depth`）は hub 側で記録する。派生の
//! 派生（孫作品）も同様に記録し、派生元の連鎖を保持する。GitHub の概念
//! （Fork 等）はアダプター層の背後に隠し、本ストアは hub 語彙のみを持つ。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use crate::ports::Derivative;
use crate::registry::{new_project_id, now_unix};

/// 共有する派生ストア。
pub type SharedDerivativeStore = Arc<DerivativeStore>;

/// インメモリの派生作品ストア。
#[derive(Debug, Default)]
pub struct DerivativeStore {
    records: RwLock<HashMap<String, Derivative>>,
    order: RwLock<Vec<String>>,
    /// Idempotency-Key → jobId（派生作成の冪等性）。
    idempotency: RwLock<HashMap<String, String>>,
}

impl DerivativeStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 派生レコードを作成する。
    pub fn add(&self, derivative: Derivative) {
        self.records
            .write()
            .unwrap()
            .insert(derivative.id.clone(), derivative.clone());
        self.order
            .write()
            .unwrap()
            .push(derivative.id.clone());
    }

    /// 派生レコードを作成し、派生元の連鎖から `depth` を決定する。
    ///
    /// `parent_id` が派生レコードとして存在する場合はその `depth + 1`、
    /// 存在しない（派生元が元作品）場合は 1 とする。
    pub fn record(
        &self,
        parent_id: &str,
        slug: &str,
        title: &str,
        owner_handle: &str,
        license: &str,
        derivative_policy: crate::ports::DerivativePolicy,
    ) -> Derivative {
        let depth = self
            .get(parent_id)
            .map(|p| p.depth + 1)
            .unwrap_or(1);
        let now = now_unix();
        let derivative = Derivative {
            id: new_project_id(),
            slug: slug.to_string(),
            title: title.to_string(),
            owner_handle: owner_handle.to_string(),
            depth,
            parent_id: Some(parent_id.to_string()),
            license: license.to_string(),
            derivative_policy,
            created_at: now,
            updated_at: now,
        };
        self.add(derivative.clone());
        derivative
    }

    pub fn get(&self, id: &str) -> Option<Derivative> {
        self.records.read().unwrap().get(id).cloned()
    }

    pub fn all(&self) -> Vec<Derivative> {
        let order = self.order.read().unwrap().clone();
        let records = self.records.read().unwrap();
        order
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .collect()
    }

    /// 指定派生元の直接の子を返す。
    pub fn children_of(&self, parent_id: &str) -> Vec<Derivative> {
        self.all()
            .into_iter()
            .filter(|d| d.parent_id.as_deref() == Some(parent_id))
            .collect()
    }

    /// Idempotency-Key で既に受理したジョブ ID を返す（無ければ None）。
    pub fn job_for_idempotency_key(&self, key: &str) -> Option<String> {
        self.idempotency.read().unwrap().get(key).cloned()
    }

    /// Idempotency-Key とジョブ ID の対応を記録する。
    pub fn remember_idempotency_key(&self, key: &str, job_id: &str) {
        self.idempotency
            .write()
            .unwrap()
            .insert(key.to_string(), job_id.to_string());
    }

    /// 指定プロジェクトの派生ツリー（派生元の連鎖＋子孫＋兄弟）を返す。
    ///
    /// プロジェクト自身の派生元を上に辿り、その連鎖上の各ノードの子孫を
    /// 集めて、一つの木（forest）として返す。元作品（派生記録が無い）の場合は
    /// その子孫のみを返す。
    pub fn tree_for(&self, project_id: &str) -> Vec<Derivative> {
        let all = self.all();
        let by_id: HashMap<&str, &Derivative> =
            all.iter().map(|d| (d.id.as_str(), d)).collect();

        // 派生元の連鎖を上に辿る（プロジェクト自身が派生記録があれば）。
        let mut chain: Vec<&str> = Vec::new();
        let mut cursor = by_id.get(project_id).and_then(|d| d.parent_id.as_deref());
        while let Some(pid) = cursor {
            if chain.contains(&pid) {
                break;
            }
            chain.push(pid);
            cursor = by_id.get(pid).and_then(|d| d.parent_id.as_deref());
        }
        // 連鎖上にプロジェクト自身も含める（子孫収集の起点）。
        chain.push(project_id);

        // 連鎖上の各ノードの子孫を収集。
        let mut collected: HashSet<String> = HashSet::new();
        for root in &chain {
            let mut stack: Vec<&str> = vec![root];
            while let Some(node) = stack.pop() {
                for child in all.iter().filter(|d| d.parent_id.as_deref() == Some(node)) {
                    if collected.insert(child.id.clone()) {
                        stack.push(child.id.as_str());
                    }
                }
            }
        }

        let mut result: Vec<Derivative> = all
            .into_iter()
            .filter(|d| collected.contains(&d.id))
            .collect();
        result.sort_by_key(|d| (d.depth, d.created_at));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::DerivativePolicy;

    fn record(store: &DerivativeStore, parent: &str, title: &str) -> Derivative {
        store.record(parent, &format!("slug-{title}"), title, "octo", "cc-by", DerivativePolicy::Allowed)
    }

    #[test]
    fn record_computes_depth_from_parent_chain() {
        let store = DerivativeStore::new();
        let child = record(&store, "root", "child");
        assert_eq!(child.depth, 1);
        assert_eq!(child.parent_id.as_deref(), Some("root"));

        let grandchild = record(&store, &child.id, "grandchild");
        assert_eq!(grandchild.depth, 2);
        assert_eq!(grandchild.parent_id.as_deref(), Some(child.id.as_str()));
    }

    #[test]
    fn tree_for_returns_connected_subtree() {
        let store = DerivativeStore::new();
        let a = record(&store, "root", "a");
        let b = record(&store, "root", "b");
        let a1 = record(&store, &a.id, "a1");
        let a11 = record(&store, &a1.id, "a11");

        // root のツリーは a, b, a1, a11 の全部。
        let root_tree = store.tree_for("root");
        let ids: Vec<&str> = root_tree.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&a.id.as_str()));
        assert!(ids.contains(&b.id.as_str()));
        assert!(ids.contains(&a1.id.as_str()));
        assert!(ids.contains(&a11.id.as_str()));

        // a のツリーは派生元連鎖（root）の子孫を含む。よって a 自身・a1・a11
        // に加え、root の子である兄弟 b も含まれる。
        let a_tree = store.tree_for(&a.id);
        let a_ids: Vec<&str> = a_tree.iter().map(|d| d.id.as_str()).collect();
        assert!(a_ids.contains(&a.id.as_str()));
        assert!(a_ids.contains(&a1.id.as_str()));
        assert!(a_ids.contains(&a11.id.as_str()));
        // 派生元連鎖の子孫として兄弟 b も含む。
        assert!(a_ids.contains(&b.id.as_str()));
    }

    #[test]
    fn tree_for_unknown_project_returns_empty() {
        let store = DerivativeStore::new();
        assert!(store.tree_for("nope").is_empty());
    }
}
