//! 派生（GitHub Fork 連携）のアダプター（設計 §2.2）。
//!
//! `DerivationPort` を実装し、GitHub の Fork を代理操作する。Fork は非同期
//! のため、fork 完了をポーリングで確認してから hub に派生レコード
//! （`parent_id` / `depth`）を作成し、完了を通知（`derivative_created`）で
//! 知らせる。GitHub の概念（Fork 等）は本モジュール内に閉じ込め、API 契約・
//! フロントエンドには漏らさない。

use std::sync::{Arc, RwLock};

use crate::adapters::github::client::{FileEncoding, GitHubClient, Token};
use crate::derivatives::SharedDerivativeStore;
use crate::notifications::SharedNotificationStore;
use crate::ports::{
    Derivative, DerivativePolicy, DerivationPort, DeriveAccepted, DeriveRequest, JobStatus,
    NotificationKind, NotificationPort, PortError,
};
use crate::registry::{new_project_id, ProjectRecord, SharedProjectRegistry};

/// 派生アダプター（GitHub Fork 連携）。
#[derive(Clone)]
pub struct DerivationAdapter {
    client: GitHubClient,
    token: Arc<RwLock<Option<Token>>>,
    projects: SharedProjectRegistry,
    derivatives: SharedDerivativeStore,
    notifications: SharedNotificationStore,
}

impl DerivationAdapter {
    /// 派生リポジトリのスナップショットを novnow 軽量検証で検査する。
    ///
    /// リポジトリの全ファイルを再帰的に取得し、`verify::verify_snapshot`
    /// （M3 の軽量検証相当）を通す。不備があれば `Err(String)` を返す。
    async fn verify_fork_snapshot(
        &self,
        token: &Token,
        full_name: &str,
        default_branch: &str,
    ) -> Result<(), String> {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;

        let mut files: Vec<crate::verify::SnapshotFile> = Vec::new();
        let mut dirs: Vec<String> = Vec::new();
        let mut stack: Vec<String> = vec![String::new()];

        while let Some(prefix) = stack.pop() {
            let entries = self
                .client
                .list_contents(token, full_name, Some(prefix.as_str()))
                .await
                .map_err(|e| format!("contents 一覧取得失敗: {e:?}"))?;
            for entry in entries {
                let path = entry.path.trim_matches('/').to_string();
                if entry.entry_type == "dir" {
                    dirs.push(entry.path.trim_matches('/').to_string());
                    stack.push(entry.path);
                    continue;
                }
                if entry.entry_type != "file" {
                    continue;
                }
                let file = self
                    .client
                    .get_file(token, full_name, &entry.path)
                    .await
                    .map_err(|e| format!("ファイル取得失敗 {path}: {e:?}"))?;
                let content = match file.encoding {
                    FileEncoding::Base64 => STANDARD
                        .decode(file.content.replace(['\\', '\n'], ""))
                        .map_err(|e| format!("Base64 復号失敗 {path}: {e}"))?,
                    FileEncoding::Text => file.content.into_bytes(),
                };
                let _ = &default_branch;
                files.push(crate::verify::SnapshotFile { path, content });
            }
        }

        crate::verify::verify_snapshot(&files, &dirs).map_err(|e| e.to_string())
    }

    pub fn new(
        client: GitHubClient,
        token: Arc<RwLock<Option<Token>>>,
        projects: SharedProjectRegistry,
        derivatives: SharedDerivativeStore,
        notifications: SharedNotificationStore,
    ) -> Self {
        Self {
            client,
            token,
            projects,
            derivatives,
            notifications,
        }
    }

    fn token(&self) -> Result<Token, PortError> {
        self.token
            .read()
            .unwrap()
            .clone()
            .ok_or(PortError::PermissionDenied)
    }

    /// Fork を実行し、完了後に hub に派生レコードを作成・登録・通知する。
    #[allow(clippy::too_many_arguments)]
    async fn complete_derivative(
        &self,
        job_id: &str,
        shell_id: &str,
        parent_project_id: &str,
        parent_owner: &str,
        license: &str,
        parent_title: &str,
        actor_handle: &str,
        req: &DeriveRequest,
    ) -> Result<(), PortError> {
        let token = self.token()?;

        // Fork を要求し、完了をポーリングで待つ。
        let fork_repo = self
            .client
            .create_fork(&token, shell_id)
            .await
            .map_err(|e| crate::adapters::github::map_client_error(&e))?;
        let fork_shell_id = fork_repo.full_name.clone();

        // 初回公開前の novnow 形式検証（M3 の軽量検証相当、設計 §2.2）。
        // Fork されたリポジトリのスナップショットを検証し、不備があれば
        // 派生レコードを作成しない（公開を止める）。
        self.verify_fork_snapshot(&token, &fork_shell_id, &fork_repo.default_branch)
            .await
            .map_err(|e| {
                tracing::warn!(
                    job_id = %job_id,
                    fork = %fork_shell_id,
                    error = %e,
                    "derivative novnow verification failed"
                );
                PortError::InvalidResponse
            })?;

        // hub に派生レコードを作成（parent_id / depth を記録）。
        let slug = crate::registry::slugify(&req.title);
        let derivative = self.derivatives.record(
            parent_project_id,
            &slug,
            &req.title,
            actor_handle,
            license,
            DerivativePolicy::Allowed,
        );

        // 派生作品をプロジェクト登録簿へ登録（派生元の連鎖を保持）。
        let project_id = derivative.id.clone();
        self.projects.register(ProjectRecord {
            project_id: project_id.clone(),
            title: req.title.clone(),
            description: req.description.clone(),
            tags: Vec::new(),
            owner_handle: actor_handle.to_string(),
            license: license.to_string(),
            visibility: req.visibility,
            shell_id: fork_shell_id,
            default_branch: fork_repo.default_branch.clone(),
            etag: fork_repo.etag.clone().unwrap_or_default(),
            created_at: derivative.created_at,
            updated_at: derivative.updated_at,
            derivative_policy: DerivativePolicy::Allowed.as_str().to_string(),
        });

        // 派生元の作者へ完了を通知（derivative_created）。
        let parent_project_id = self
            .projects
            .get_by_shell(shell_id)
            .map(|r| r.project_id)
            .unwrap_or_default();
        self.notifications
            .emit(
                parent_owner,
                NotificationKind::DerivativeCreated,
                Some(&parent_project_id),
                Some(shell_id),
                &req.title,
                &format!("{parent_title} の派生作品が作成されました"),
            )
            .await?;

        tracing::info!(
            job_id = %job_id,
            project_id = %project_id,
            parent = %shell_id,
            "derivative created"
        );
        Ok(())
    }
}

impl DerivationPort for DerivationAdapter {
    fn create_derivative(
        &self,
        parent_project_id: &str,
        actor_id: &str,
        actor_handle: &str,
        req: &DeriveRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<DeriveAccepted, PortError>> + Send + '_,
        >,
    > {
        let this = self.clone();
        let parent_project_id = parent_project_id.to_string();
        let actor_id = actor_id.to_string();
        let actor_handle = actor_handle.to_string();
        let req = req.clone();
        Box::pin(async move {
            // 派生元プロジェクトを解決。
            let parent = this
                .projects
                .get(&parent_project_id)
                .ok_or(PortError::NotFound)?;

            // 派生可否の方針を確認（派生不可は 409）。
            let policy = DerivativePolicy::from_policy_str(&parent.derivative_policy);
            if policy == DerivativePolicy::Disallowed {
                return Err(PortError::Conflict);
            }

            // トークンを取得（Fork 操作に必要）。
            let _token = this.token()?;

            // 派生ジョブ ID を採番し、非同期で Fork 完了を待つ。
            let job_id = new_project_id();
            let job_id_spawn = job_id.clone();
            let shell_id = parent.shell_id.clone();
            let parent_owner = parent.owner_handle.clone();
            let license = parent.license.clone();
            let parent_title = parent.title.clone();

            tokio::spawn(async move {
                if let Err(err) = this
                    .complete_derivative(
                        &job_id_spawn,
                        &shell_id,
                        &parent_project_id,
                        &parent_owner,
                        &license,
                        &parent_title,
                        &actor_handle,
                        &req,
                    )
                    .await
                {
                    tracing::error!(
                        job_id = %job_id_spawn,
                        actor = %actor_id,
                        error = %err,
                        "derivative creation failed"
                    );
                }
            });

            Ok(DeriveAccepted {
                job_id,
                status: JobStatus::Pending,
            })
        })
    }

    fn list_derivatives(
        &self,
        project_id: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<Derivative>, PortError>> + Send + '_>,
    > {
        let derivatives = self.derivatives.clone();
        let project_id = project_id.to_string();
        Box::pin(async move { Ok(derivatives.tree_for(&project_id)) })
    }

    fn get_derivation_tree(
        &self,
        project_id: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<Derivative>, PortError>> + Send + '_>,
    > {
        let derivatives = self.derivatives.clone();
        let project_id = project_id.to_string();
        Box::pin(async move { Ok(derivatives.tree_for(&project_id)) })
    }

    fn get_derivative(
        &self,
        _project_id: &str,
        derivative_id: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Derivative, PortError>> + Send + '_>,
    > {
        let derivatives = self.derivatives.clone();
        let derivative_id = derivative_id.to_string();
        Box::pin(async move {
            derivatives
                .get(&derivative_id)
                .ok_or(PortError::NotFound)
        })
    }
}
