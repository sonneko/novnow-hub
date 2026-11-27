//! 公開前検証（novnow 点検）の同期検証サービス（設計 §6.1）。
//!
//! `POST /projects/{projectId}/verify`（`verifyProject`）はプロジェクトの
//! スナップショットを読み取り、novnow 軽量検証（`verify::verify_snapshot`）を
//! 同期で実行して `VerificationResult` を返す。結果は `VerificationStore` に
//! プロジェクトごとに保持し、`GET /projects/{projectId}/verify/latest`
//! （`getLatestVerification`）で直近結果を取り出せる。
//!
//! 重い診断（novnow check 相当の全項目）は既存の非同期ジョブ（`JobPort`、
//! 設計 §4.1）で実行する。本モジュールの検証は軽量で同期的に行い、提案を
//! ブロックしすぎない（設計 §6.1）。GitHub へのアクセスは `JobContentPort`
//! の背後に隠す。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::jobs::JobContentPort;
use crate::ports::{DiagnosticItem, DiagnosticsResult, PortError, TreeEntryKind};
use crate::registry::SharedProjectRegistry;
use crate::verify::{SnapshotFile, verify_snapshot};

/// プロジェクトごとの直近検証結果のインメモリストア。
#[derive(Debug, Default)]
pub struct VerificationStore {
    results: RwLock<HashMap<String, DiagnosticsResult>>,
}

impl VerificationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 検証結果を保存する（直近のみ保持）。
    pub fn record(&self, project_id: &str, result: DiagnosticsResult) {
        self.results
            .write()
            .unwrap()
            .insert(project_id.to_string(), result);
    }

    /// プロジェクトの直近検証結果を返す。未検証なら `None`。
    pub fn latest(&self, project_id: &str) -> Option<DiagnosticsResult> {
        self.results
            .read()
            .unwrap()
            .get(project_id)
            .cloned()
    }
}

/// 公開前検証サービス。スナップショット読み取り＋novnow 軽量検証を担う。
pub struct VerificationService {
    content: Arc<dyn JobContentPort>,
    projects: SharedProjectRegistry,
    store: Arc<VerificationStore>,
}

impl VerificationService {
    pub fn new(
        content: Arc<dyn JobContentPort>,
        projects: SharedProjectRegistry,
        store: Arc<VerificationStore>,
    ) -> Self {
        Self {
            content,
            projects,
            store,
        }
    }

    /// プロジェクトのスナップショットを再帰的に読み取る（パス → 内容）。
    async fn read_snapshot(
        &self,
        shell_id: &str,
    ) -> Result<(Vec<SnapshotFile>, Vec<String>), PortError> {
        let mut files: Vec<SnapshotFile> = Vec::new();
        let mut dirs: Vec<String> = Vec::new();
        let mut stack: Vec<String> = vec![String::new()];

        while let Some(prefix) = stack.pop() {
            let entries = self
                .content
                .list_tree(shell_id, Some(prefix.as_str()))
                .await?;
            for entry in entries {
                let path = entry.path.trim_matches('/').to_string();
                if entry.kind == TreeEntryKind::Directory {
                    dirs.push(entry.path.trim_matches('/').to_string());
                    stack.push(entry.path);
                    continue;
                }
                if entry.kind != TreeEntryKind::File {
                    continue;
                }
                let file = self.content.read_file(shell_id, &entry.path).await?;
                files.push(SnapshotFile {
                    path,
                    content: file.content.into_bytes(),
                });
            }
        }
        Ok((files, dirs))
    }

    /// novnow 軽量検証を同期で実行し、結果を保存して返す。
    ///
    /// 検証エラー（`ok == false`）でも検証自体は完了した扱いで結果を返す
    /// （ジョブの診断と同様、エラーを結果に残す）。保存は常に行う。
    pub async fn verify(
        &self,
        project_id: &str,
    ) -> Result<DiagnosticsResult, PortError> {
        let shell_id = self
            .projects
            .get(project_id)
            .ok_or(PortError::NotFound)?
            .shell_id
            .clone();
        let (files, dirs) = self.read_snapshot(&shell_id).await?;
        let checked_at = crate::registry::now_unix();
        let result = match verify_snapshot(&files, &dirs) {
            Ok(()) => DiagnosticsResult {
                ok: true,
                errors: Vec::new(),
                warnings: Vec::new(),
                checked_at,
            },
            Err(verr) => {
                let errors = verr
                    .reasons
                    .iter()
                    .map(|r| DiagnosticItem {
                        path: String::new(),
                        code: "novnow_check".to_string(),
                        message: r.clone(),
                    })
                    .collect();
                DiagnosticsResult {
                    ok: false,
                    errors,
                    warnings: Vec::new(),
                    checked_at,
                }
            }
        };
        self.store.record(project_id, result.clone());
        Ok(result)
    }

    /// プロジェクトの直近検証結果を返す。未検証なら `None`。
    pub fn latest(&self, project_id: &str) -> Option<DiagnosticsResult> {
        self.store.latest(project_id)
    }
}

/// 共有する検証サービスハンドル。
pub type SharedVerificationService = Arc<VerificationService>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::JobContentPort;
    use crate::ports::{ContentPort, FileContent, NewFile, TreeEntry};

    /// テスト用のモック ContentPort（固定のファイル一覧を返す）。
    struct MockContent {
        files: Vec<(String, String)>,
    }

    impl ContentPort for MockContent {
        async fn list_tree(
            &self,
            _shell_id: &str,
            path: Option<&str>,
        ) -> Result<Vec<TreeEntry>, PortError> {
            let prefix = path.unwrap_or("").trim_matches('/');
            let mut entries = Vec::new();
            for (p, _) in &self.files {
                if prefix.is_empty() {
                    if p.contains('/') {
                        // 先頭のディレクトリ区切りをディレクトリ entry として返す。
                        let dir = p.split('/').next().unwrap_or("");
                        entries.push(TreeEntry {
                            path: dir.to_string(),
                            kind: TreeEntryKind::Directory,
                            size: None,
                            etag: String::new(),
                        });
                    } else {
                        entries.push(TreeEntry {
                            path: p.clone(),
                            kind: TreeEntryKind::File,
                            size: None,
                            etag: String::new(),
                        });
                    }
                } else if p.starts_with(&format!("{prefix}/")) {
                    let rest = &p[prefix.len() + 1..];
                    if rest.contains('/') {
                        let sub = rest.split('/').next().unwrap_or("");
                        entries.push(TreeEntry {
                            path: format!("{prefix}/{sub}"),
                            kind: TreeEntryKind::Directory,
                            size: None,
                            etag: String::new(),
                        });
                    } else {
                        entries.push(TreeEntry {
                            path: p.clone(),
                            kind: TreeEntryKind::File,
                            size: None,
                            etag: String::new(),
                        });
                    }
                }
            }
            entries.dedup_by(|a, b| a.path == b.path);
            Ok(entries)
        }

        async fn read_file(&self, _shell_id: &str, path: &str) -> Result<FileContent, PortError> {
            let content = self
                .files
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, c)| c.clone())
                .unwrap_or_default();
            Ok(FileContent {
                content,
                encoding: crate::ports::FileEncoding::Text,
                etag: String::new(),
            })
        }

        async fn commit_files(
            &self,
            _shell_id: &str,
            _branch: &str,
            _files: &[NewFile],
        ) -> Result<String, PortError> {
            Ok(String::new())
        }

        async fn get_etag(
            &self,
            _shell_id: &str,
            _path: Option<&str>,
        ) -> Result<String, PortError> {
            Ok(String::new())
        }
    }

    fn valid_files() -> Vec<(String, String)> {
        vec![
            ("project.toml".into(), "schema_version = 1\n".into()),
            ("characters.md".into(), "# characters\n".into()),
            ("words.md".into(), "# words\n".into()),
            ("materials/index.json".into(), "{}".into()),
            ("plots/all.cnov".into(), "{}".into()),
            ("contents/01.cnov".into(), "body\n".into()),
        ]
    }

    fn build_service(files: Vec<(String, String)>) -> (Arc<VerificationService>, Arc<VerificationStore>) {
        let store = Arc::new(VerificationStore::new());
        let content = Arc::new(MockContent { files }) as Arc<dyn JobContentPort>;
        let projects = crate::registry::ProjectRegistry::new();
        use crate::registry::{ProjectRecord, DEFAULT_BRANCH};
        use crate::ports::ProjectVisibility;
        projects.register(ProjectRecord {
            project_id: "prj".into(),
            title: "Prj".into(),
            description: None,
            tags: Vec::new(),
            owner_handle: "owner".into(),
            license: "cc-by".into(),
            derivative_policy: "allowed".into(),
            visibility: ProjectVisibility::Public,
            shell_id: "shell-prj".into(),
            default_branch: DEFAULT_BRANCH.to_string(),
            etag: "etag".into(),
            created_at: 0,
            updated_at: 0,
        });
        let service = Arc::new(VerificationService::new(content, Arc::new(projects), store.clone()));
        (service, store)
    }

    #[tokio::test]
    async fn verify_succeeds_on_valid_project() {
        let (service, _store) = build_service(valid_files());
        let result = service.verify("prj").await.unwrap();
        assert!(result.ok);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn verify_reports_errors_on_invalid_project() {
        let files = vec![("project.toml".into(), "schema_version = 1\n".into())];
        let (service, _store) = build_service(files);
        let result = service.verify("prj").await.unwrap();
        assert!(!result.ok);
        assert!(!result.errors.is_empty());
    }

    #[tokio::test]
    async fn verify_records_and_latest_returns() {
        let (service, store) = build_service(valid_files());
        // 未検証は None。
        assert!(service.latest("prj").is_none());
        let result = service.verify("prj").await.unwrap();
        // 検証後は保存済み。
        assert!(service.latest("prj").is_some());
        assert_eq!(store.latest("prj").unwrap().ok, result.ok);
    }

    #[tokio::test]
    async fn verify_unknown_project_is_not_found() {
        let (service, _store) = build_service(valid_files());
        let err = service.verify("nope").await.unwrap_err();
        assert!(matches!(err, PortError::NotFound));
    }
}
