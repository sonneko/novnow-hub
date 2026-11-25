//! novnow 診断・エクスポートの非同期ジョブ基盤（設計 §4.1）。
//!
//! `JobStore` がジョブ記録をインメモリで保持し、`JobManager` が `JobPort`
//! を実装する。重い処理（診断・エクスポート）はキュー化され、同時実行数
//! を `JOB_CONCURRENCY_LIMIT` で制限する。完了は通知（`job_finished`）で
//! 知らせる。GitHub の概念（GitHub Actions 等）は実行環境の背後に隠す。

/// 共有するジョブアダプター（`JobPort` 実装）。
pub type SharedJobPort = std::sync::Arc<dyn crate::ports::JobPort>;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::future::Future;

use crate::ports::{
    ContentPort, CreateJobRequest, NotificationPort, DiagnosticItem, DiagnosticsResult, ExportFormat, ExportResult,
    Job, FileContent, JobKind, JobPort, JobResult, JobState, PortError, TreeEntry, TreeEntryKind,
};
use crate::registry::{new_project_id, now_unix, SharedProjectRegistry};
use crate::verify::{SnapshotFile, verify_snapshot};

/// 同時実行ジョブ数の上限（設計 §4.1「同時実行数を制限する」）。
///
/// 重い処理（診断・エクスポート）を並行しすぎないため、上限を定数で定義する。
pub const JOB_CONCURRENCY_LIMIT: usize = 2;

/// ジョブの実行者が読むコンテンツのポート（`dyn` 互換）。
///
/// `ContentPort` は `impl Future` のため `dyn` 非互換。ジョブ実行者は
/// コンテンツ読み取りのみを必要とするため、ボックス化 future の専用ポート
/// を置き、`ContentPort` 実装者に対して blanket impl で自動的に提供する。
pub trait JobContentPort: Send + Sync {
    fn list_tree<'a>(
        &'a self,
        shell_id: &'a str,
        path: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TreeEntry>, PortError>> + Send + 'a>>;

    fn read_file<'a>(
        &'a self,
        shell_id: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<FileContent, PortError>> + Send + 'a>>;
}

impl<T: ContentPort + Send + Sync> JobContentPort for T {
    fn list_tree<'a>(
        &'a self,
        shell_id: &'a str,
        path: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TreeEntry>, PortError>> + Send + 'a>> {
        Box::pin(self.list_tree(shell_id, path))
    }

    fn read_file<'a>(
        &'a self,
        shell_id: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<FileContent, PortError>> + Send + 'a>> {
        Box::pin(self.read_file(shell_id, path))
    }
}

/// インメモリのジョブストア。
#[derive(Debug, Default)]
pub struct JobStore {
    jobs: RwLock<HashMap<String, Job>>,
    order: RwLock<Vec<String>>,
}

impl JobStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// ジョブを起票する（状態は Pending）。
    pub fn enqueue(&self, kind: JobKind, _project_id: &str) -> Job {
        let now = now_unix();
        let job = Job {
            id: new_project_id(),
            kind,
            status: JobState::Pending,
            result: None,
            error: None,
            created_at: now,
            finished_at: None,
        };
        self.jobs
            .write()
            .unwrap()
            .insert(job.id.clone(), job.clone());
        self.order
            .write()
            .unwrap()
            .push(job.id.clone());
        job
    }

    pub fn get(&self, job_id: &str) -> Option<Job> {
        self.jobs.read().unwrap().get(job_id).cloned()
    }

    pub fn list(&self) -> Vec<Job> {
        let order = self.order.read().unwrap().clone();
        let jobs = self.jobs.read().unwrap();
        order
            .iter()
            .filter_map(|id| jobs.get(id).cloned())
            .collect()
    }

    /// 状態と結果を更新する。
    pub fn finish(
        &self,
        job_id: &str,
        status: JobState,
        result: Option<JobResult>,
        error: Option<String>,
    ) {
        if let Some(job) = self.jobs.write().unwrap().get_mut(job_id) {
            job.status = status;
            job.result = result;
            job.error = error;
            if status.is_finished() {
                job.finished_at = Some(now_unix());
            }
        }
    }

    /// ジョブを実行中へ遷移する。
    pub fn mark_running(&self, job_id: &str) {
        if let Some(job) = self.jobs.write().unwrap().get_mut(job_id) {
            if job.status == JobState::Pending {
                job.status = JobState::Running;
            }
        }
    }

    /// ジョブをキャンセルする（完了済みはキャンセルできない）。
    pub fn cancel(&self, job_id: &str) -> Option<Job> {
        let mut jobs = self.jobs.write().unwrap();
        let job = jobs.get_mut(job_id)?;
        if job.status.is_finished() {
            return Some(job.clone());
        }
        job.status = JobState::Cancelled;
        job.error = Some("キャンセルされました".to_string());
        job.finished_at = Some(now_unix());
        Some(job.clone())
    }
}

/// ジョブの実行者（診断・エクスポート）。
///
/// `JobContentPort` でプロジェクトのコンテンツを読み、診断は novnow 形式検証
/// （M3 軽量検証相当）を、エクスポートは本文の注釈除去済みテキスト／
/// Markdown を生成する。
pub struct JobRunner {
    content: Arc<dyn JobContentPort>,
    projects: SharedProjectRegistry,
}

impl JobRunner {
    pub fn new(content: Arc<dyn JobContentPort>, projects: SharedProjectRegistry) -> Self {
        Self { content, projects }
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
            let entries = self.content.list_tree(shell_id, Some(prefix.as_str())).await?;
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

    /// 診断（novnow check 相当）を実行する。
    ///
    /// 本段階では novnow の WASM 実行は現実的でない（novnow WASM モジュールが
    /// ベンダー未収録・オフラインのため）。代わりに M3 の軽量検証
    /// （`verify::verify_snapshot`、novnow 形式の必須ファイル・UTF-8・マニフェスト
    /// の整合性検査）を診断の最小実装として再利用する（判断は `docs/handoff.md`
    /// §4 に記録）。hub は novnow 本体の全機能を再実装しない。
    async fn run_diagnostics(&self, shell_id: &str) -> Result<DiagnosticsResult, PortError> {
        let (files, dirs) = self.read_snapshot(shell_id).await?;
        let checked_at = now_unix();
        match verify_snapshot(&files, &dirs) {
            Ok(()) => Ok(DiagnosticsResult {
                ok: true,
                errors: Vec::new(),
                warnings: Vec::new(),
                checked_at,
            }),
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
                Ok(DiagnosticsResult {
                    ok: false,
                    errors,
                    warnings: Vec::new(),
                    checked_at,
                })
            }
        }
    }

    /// エクスポート（注釈除去済みテキスト／Markdown）を実行する。
    async fn run_export(
        &self,
        shell_id: &str,
        format: ExportFormat,
    ) -> Result<ExportResult, PortError> {
        let (files, _dirs) = self.read_snapshot(shell_id).await?;
        // 文書ファイル（.md / .cnov / .txt / .json）を集約し、注釈を除いた
        // 本文を生成する。Phase 1 では内容をそのまま連結し、形式ごとに整形する。
        let mut doc_files: Vec<&SnapshotFile> = files
            .iter()
            .filter(|f| {
                matches!(
                    f.path.rsplit('.').next(),
                    Some("md") | Some("cnov") | Some("txt") | Some("json")
                )
            })
            .collect();
        doc_files.sort_by(|a, b| a.path.cmp(&b.path));

        let mut body = String::new();
        for f in doc_files {
            if format == ExportFormat::Markdown {
                body.push_str(&format!("## {}\n\n", f.path));
            }
            if let Ok(text) = std::str::from_utf8(&f.content) {
                body.push_str(text);
                body.push('\n');
            }
        }
        Ok(ExportResult {
            format: format.as_str().to_string(),
            char_count: body.chars().count(),
        })
    }
}

/// `JobPort` の実装。ジョブの起票・状態取得・キャンセルを扱い、実行は
/// 非同期タスクでキュー化（`tokio::spawn` でバックグラウンド実行）する。
pub struct JobManager {
    store: Arc<JobStore>,
    runner: Arc<JobRunner>,
    notifications: crate::notifications::SharedNotificationStore,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl JobManager {
    pub fn new(
        store: Arc<JobStore>,
        runner: Arc<JobRunner>,
        notifications: crate::notifications::SharedNotificationStore,
    ) -> Self {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(JOB_CONCURRENCY_LIMIT));
        Self {
            store,
            runner,
            notifications,
            semaphore,
        }
    }

    /// ジョブを起票し、実行をバックグラウンドタスクへ委譲して即座に返る。
    fn spawn_execution(
        &self,
        job_id: String,
        project_id: String,
        kind: JobKind,
        format: Option<ExportFormat>,
    ) {
        let store = self.store.clone();
        let runner = self.runner.clone();
        let notifications = self.notifications.clone();
        let semaphore = self.semaphore.clone();
        tokio::spawn(async move {
            // 同時実行数の上限を超えないようセマフォを待つ。
            let _permit = semaphore.acquire_owned().await.ok();
            let mgr = JobRunnerHandle {
                store,
                runner,
                notifications,
            };
            mgr.execute(job_id, project_id, kind, format).await;
        });
    }
}

/// バックグラウンドでジョブを実行するハンドル（スパウンされたタスク内）。
struct JobRunnerHandle {
    store: Arc<JobStore>,
    runner: Arc<JobRunner>,
    notifications: crate::notifications::SharedNotificationStore,
}

impl JobRunnerHandle {
    async fn execute(
        &self,
        job_id: String,
        project_id: String,
        kind: JobKind,
        format: Option<ExportFormat>,
    ) {
        self.store.mark_running(&job_id);

        let result = self.execute_inner(&job_id, &project_id, kind, format).await;

        match result {
            Ok((status, job_result)) => {
                self.store
                    .finish(&job_id, status, Some(job_result.clone()), None);
                tracing::info!(job_id = %job_id, kind = kind.as_str(), "job succeeded");
                self.notify_finished(&job_id, &project_id, true, None).await;
            }
            Err(err) => {
                let msg = err.to_string();
                self.store
                    .finish(&job_id, JobState::Failed, None, Some(msg.clone()));
                tracing::error!(job_id = %job_id, error = %msg, "job failed");
                self.notify_finished(&job_id, &project_id, false, Some(&msg)).await;
            }
        }
    }

    async fn execute_inner(
        &self,
        _job_id: &str,
        project_id: &str,
        kind: JobKind,
        format: Option<ExportFormat>,
    ) -> Result<(JobState, JobResult), PortError> {
        let shell_id = self
            .runner
            .projects
            .get(project_id)
            .ok_or(PortError::NotFound)?
            .shell_id
            .clone();

        match kind {
            JobKind::Diagnostics | JobKind::Verify => {
                let result = self.runner.run_diagnostics(&shell_id).await?;
                Ok((JobState::Succeeded, JobResult::Diagnostics(result)))
            }
            JobKind::Export => {
                let fmt = format.unwrap_or(ExportFormat::Txt);
                let result = self.runner.run_export(&shell_id, fmt).await?;
                Ok((JobState::Succeeded, JobResult::Export(result)))
            }
        }
    }

    async fn notify_finished(
        &self,
        job_id: &str,
        project_id: &str,
        succeeded: bool,
        error: Option<&str>,
    ) {
        let title = if succeeded {
            "ジョブが完了しました".to_string()
        } else {
            "ジョブが失敗しました".to_string()
        };
        let body = match error {
            Some(e) => format!("ジョブ {job_id} が失敗しました: {e}"),
            None => format!("ジョブ {job_id} が完了しました"),
        };
        // 操作者への通知はシステム层面で発火する（依頼者の ID は作成側で
        // 渡すが、ここではプロジェクトに対するシステム通知として記録する）。
        let _ = self.notifications.emit(
            "system",
            crate::ports::NotificationKind::JobFinished,
            Some(project_id),
            None,
            &title,
            &body,
        )
        .await;
    }
}

impl JobPort for JobManager {
    fn create_job(
        &self,
        req: &CreateJobRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Job, PortError>> + Send + '_>> {
        let job = self.store.enqueue(req.kind, &req.project_id);
        let job_id = job.id.clone();
        let project_id = req.project_id.clone();
        let kind = req.kind;
        let format = req.format;
        let this = self;
        Box::pin(async move {
            // 重い処理はバックグラウンドタスクに委譲し、起票は即座に返る。
            this.spawn_execution(job_id, project_id, kind, format);
            Ok(job)
        })
    }

    fn get_job(
        &self,
        job_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Job, PortError>> + Send + '_>> {
        let store = self.store.clone();
        let job_id = job_id.to_string();
        Box::pin(async move { store.get(&job_id).ok_or(PortError::NotFound) })
    }

    fn list_jobs(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Job>, PortError>> + Send + '_>> {
        let store = self.store.clone();
        Box::pin(async move { Ok(store.list()) })
    }

    fn cancel_job(
        &self,
        job_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Job, PortError>> + Send + '_>> {
        let store = self.store.clone();
        let job_id = job_id.to_string();
        Box::pin(async move { store.cancel(&job_id).ok_or(PortError::NotFound) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                let parent = p.rsplit('/').next().unwrap_or("");
                if prefix.is_empty() {
                    if p.contains('/') {
                        entries.push(TreeEntry {
                            path: parent.to_string(),
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
            _files: &[crate::ports::NewFile],
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
        ]
    }

    fn build_manager(files: Vec<(String, String)>) -> (Arc<JobManager>, Arc<JobStore>) {
        let store = Arc::new(JobStore::new());
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
        let runner = Arc::new(JobRunner::new(content, Arc::new(projects)));
        let notifications = Arc::new(crate::notifications::NotificationStore::new());
        let manager = Arc::new(JobManager::new(store.clone(), runner, notifications));
        (manager, store)
    }

    #[tokio::test]
    async fn diagnostics_job_succeeds_on_valid_project() {
        let (manager, store) = build_manager(valid_files());
        let job = manager
            .create_job(&CreateJobRequest {
                kind: JobKind::Diagnostics,
                project_id: "prj".into(),
                format: None,
            })
            .await
            .unwrap();
        assert_eq!(job.status, JobState::Pending);

        for _ in 0..50 {
            if let Some(j) = store.get(&job.id) {
                if j.status.is_finished() {
                    assert_eq!(j.status, JobState::Succeeded);
                    assert!(matches!(j.result, Some(JobResult::Diagnostics(_))));
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("ジョブが完了しなかった");
    }

    #[tokio::test]
    async fn diagnostics_job_reports_errors_on_invalid_project() {
        let files = vec![("project.toml".into(), "schema_version = 1\n".into())];
        let (manager, store) = build_manager(files);
        let job = manager
            .create_job(&CreateJobRequest {
                kind: JobKind::Diagnostics,
                project_id: "prj".into(),
                format: None,
            })
            .await
            .unwrap();
        for _ in 0..50 {
            if let Some(j) = store.get(&job.id) {
                if j.status.is_finished() {
                    // novnow 形式検証がエラーを返すが、ジョブ自体は診断を
                    // 完了した扱いで Succeeded（結果に errors を残す）。
                    assert_eq!(j.status, JobState::Succeeded);
                    if let Some(JobResult::Diagnostics(r)) = j.result {
                        assert!(!r.ok);
                        assert!(!r.errors.is_empty());
                    }
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("ジョブが完了しなかった");
    }

    #[tokio::test]
    async fn export_job_generates_result() {
        let (manager, store) = build_manager(valid_files());
        let job = manager
            .create_job(&CreateJobRequest {
                kind: JobKind::Export,
                project_id: "prj".into(),
                format: Some(ExportFormat::Markdown),
            })
            .await
            .unwrap();
        for _ in 0..50 {
            if let Some(j) = store.get(&job.id) {
                if j.status.is_finished() {
                    assert_eq!(j.status, JobState::Succeeded);
                    if let Some(JobResult::Export(r)) = j.result {
                        assert_eq!(r.format, "markdown");
                    }
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("ジョブが完了しなかった");
    }

    #[tokio::test]
    async fn get_and_list_jobs() {
        let (manager, _store) = build_manager(valid_files());
        let job = manager
            .create_job(&CreateJobRequest {
                kind: JobKind::Diagnostics,
                project_id: "prj".into(),
                format: None,
            })
            .await
            .unwrap();
        let fetched = manager.get_job(&job.id).await.unwrap();
        assert_eq!(fetched.id, job.id);
        let list = manager.list_jobs().await.unwrap();
        assert!(list.iter().any(|j| j.id == job.id));
    }

    #[tokio::test]
    async fn cancel_job_marks_cancelled() {
        let (manager, _store) = build_manager(valid_files());
        let job = manager
            .create_job(&CreateJobRequest {
                kind: JobKind::Diagnostics,
                project_id: "prj".into(),
                format: None,
            })
            .await
            .unwrap();
        let cancelled = manager.cancel_job(&job.id).await.unwrap();
        assert_eq!(cancelled.status, JobState::Cancelled);
        let again = manager.cancel_job(&job.id).await.unwrap();
        assert_eq!(again.status, JobState::Cancelled);
    }

    #[tokio::test]
    async fn unknown_job_is_not_found() {
        let (manager, _store) = build_manager(valid_files());
        let err = manager.get_job("nope").await.unwrap_err();
        assert!(matches!(err, PortError::NotFound));
    }
}
