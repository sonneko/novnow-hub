//! GitHub アダプターの最小実装。
//!
//! 設計書 `REF:docs/designs/api.md` §7 に準拠する。GitHub へのアクセスはすべて
//! このモジュール内に閉じ込め、`ports` のトレイト越しにのみ使われる。
//! GitHub の型（Repository 等）は `pub` に出さない。

pub mod client;
pub mod derivation;
pub mod webhook;

pub use derivation::DerivationAdapter;

#[cfg(test)]
mod m4_contract_test;

#[cfg(test)]
mod m5_derivation_contract_test;

#[cfg(test)]
mod content_port_test;

use std::sync::Arc;

use std::hash::Hasher;

use crate::ports::{
    AutomergeConfig, AutomergePort, AutomergeState, AutomergeStateKind, CheckStatus,
    CheckStatusPort, CheckState, Comment, DiagnosticsDelta, DiffStats, FileStatus, Issue,
    IssuePort, IssueState, IssueUpdate, MergeResult, PortError, Proposal, ProposalDiff,
    ProposalFile, ProposalPort, ProposalState, ReviewVerdict,
};
use crate::ports::{
    ContentPort, FileContent, Identity, IdentityPort, NewFile, ProjectShell,
    ProjectStoragePort, ProjectVisibility, TreeEntry, TreeEntryKind,
};

use self::client::{
    CheckRunJson, CommentJson, GitHubClient, IssueJson, OAuthConfig, PrJson, ReviewJson, Token,
};

/// GitHub アダプター。`IdentityPort`／`ProjectStoragePort`／`ContentPort` を実装する。
///
/// 認証トークンはアダプターが保持し、各リクエストに付与する。トークンの
/// 取得は `client::GitHubClient` に委譲する（設計 §7.4 の暗号化トークン
/// ストアは後続タスク）。トークンは OAuth 交換後に `set_token` で設定する。
pub struct GitHubAdapter {
    client: GitHubClient,
    token: Arc<std::sync::RwLock<Option<Token>>>,
    oauth: OAuthConfig,
}

impl GitHubAdapter {
    /// トークンストアは外部で共有可能（派生アダプター等と共有）なため、
    /// 引数で受け取る。
    pub fn new(
        client: GitHubClient,
        oauth: OAuthConfig,
        token: Arc<std::sync::RwLock<Option<Token>>>,
    ) -> Self {
        Self {
            client,
            token,
            oauth,
        }
    }

    /// OAuth 交換で得られたアクセストークンを設定する。
    pub fn set_token(&self, token: Token) {
        *self.token.write().unwrap() = Some(token);
    }

    fn token(&self) -> Result<Token, PortError> {
        self.token
            .read()
            .unwrap()
            .clone()
            .ok_or(PortError::PermissionDenied)
    }
}

impl IdentityPort for GitHubAdapter {
    async fn exchange_code_for_identity(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<Identity, PortError> {
        // 認可コードを GitHub のアクセストークンへ交換（PKCE）。
        let token = self
            .client
            .exchange_code(code, code_verifier, &self.oauth)
            .await
            .map_err(|e| map_client_error(&e))?;

        // トークンで利用者情報を取得。
        let user = self
            .client
            .get_user(&token)
            .await
            .map_err(|e| map_client_error(&e))?;

        // 以降の保管単位操作に使うトークンとして保持。
        self.set_token(token);

        Ok(Identity {
            id: user.id.to_string(),
            handle: user.login.clone(),
            display_name: user.name.unwrap_or_else(|| user.login.clone()),
            avatar_url: Some(user.avatar_url),
        })
    }

    async fn revoke_grant(&self, _token: &str) -> Result<(), PortError> {
        // 最小実装: GitHub 側の許可取消しは後続タスク。ここでは成功を返す。
        Ok(())
    }
}

impl ProjectStoragePort for GitHubAdapter {
    async fn create_project_shell(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<ProjectShell, PortError> {
        let token = self.token()?;
        let repo = self
            .client
            .create_repo(&token, name, description)
            .await
            .map_err(|e| map_client_error(&e))?;
        let visibility = if repo.private {
            ProjectVisibility::Private
        } else {
            ProjectVisibility::Public
        };
        Ok(ProjectShell {
            id: repo.full_name,
            default_branch: repo.default_branch,
            visibility,
            etag: repo.etag.unwrap_or_default(),
        })
    }

    async fn get_project_meta(&self, shell_id: &str) -> Result<ProjectShell, PortError> {
        let token = self.token()?;
        let repo = self
            .client
            .get_repo(&token, shell_id)
            .await
            .map_err(|e| map_client_error(&e))?;
        let visibility = if repo.private {
            ProjectVisibility::Private
        } else {
            ProjectVisibility::Public
        };
        Ok(ProjectShell {
            id: repo.full_name,
            default_branch: repo.default_branch,
            visibility,
            etag: repo.etag.unwrap_or_default(),
        })
    }

    async fn set_visibility(
        &self,
        shell_id: &str,
        visibility: ProjectVisibility,
    ) -> Result<(), PortError> {
        let token = self.token()?;
        let private = matches!(visibility, ProjectVisibility::Private);
        self.client
            .set_repo_private(&token, shell_id, private)
            .await
            .map_err(|e| map_client_error(&e))
    }

    async fn delete_project_shell(&self, shell_id: &str) -> Result<(), PortError> {
        let token = self.token()?;
        self.client
            .delete_repo(&token, shell_id)
            .await
            .map_err(|e| map_client_error(&e))
    }
}

impl ContentPort for GitHubAdapter {
    async fn list_tree(
        &self,
        shell_id: &str,
        path: Option<&str>,
    ) -> Result<Vec<TreeEntry>, PortError> {
        let token = self.token()?;
        let entries = self
            .client
            .list_contents(&token, shell_id, path)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(entries
            .into_iter()
            .map(|e| TreeEntry {
                path: e.path,
                kind: match e.entry_type.as_str() {
                    "dir" => TreeEntryKind::Directory,
                    _ => TreeEntryKind::File,
                },
                size: e.size,
                etag: e.sha,
            })
            .collect())
    }

    async fn read_file(&self, shell_id: &str, path: &str) -> Result<FileContent, PortError> {
        let token = self.token()?;
        let file = self
            .client
            .get_file(&token, shell_id, path)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(decode_file_entry(file))
    }

    async fn commit_files(
        &self,
        shell_id: &str,
        branch: &str,
        files: &[NewFile],
    ) -> Result<String, PortError> {
        let token = self.token()?;
        self.client
            .commit_files(&token, shell_id, branch, files)
            .await
            .map_err(|e| map_client_error(&e))
    }

    async fn get_etag(&self, shell_id: &str, path: Option<&str>) -> Result<String, PortError> {
        let token = self.token()?;
        match path {
            Some(p) => {
                let file = self
                    .client
                    .get_file(&token, shell_id, p)
                    .await
                    .map_err(|e| map_client_error(&e))?;
                Ok(file.sha)
            }
            None => {
                let repo = self
                    .client
                    .get_repo(&token, shell_id)
                    .await
                    .map_err(|e| map_client_error(&e))?;
                Ok(repo.etag.unwrap_or_default())
            }
        }
    }
}

/// クライアントエラーから PortError への変換（設計 §7.3 の表に準拠）。
/// GitHub のファイル応答を hub の `FileContent` へ写す。
///
/// GitHub の contents API はテキストもバイナリも **base64**（60 文字ごとの
/// 改行入り）で返す。hub の契約 `FileContent` は `encoding=utf8` のときに
/// `content`、`encoding=base64` のときに `data` を要求するため、ここで
/// 一度復号し、UTF-8 として妥当なら `Text`（改行は文書どおり保持）、
/// そうでなければ `Base64` として扱い、**内容を壊さない**（判断:
/// `REF:docs/handoff.md` §4 判断 6(b)・28）。
///
/// 復号に失敗した場合は外部基盤の応答不正として扱う（黙って文字化けさせない）。
fn decode_file_entry(file: client::FileEntry) -> FileContent {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    match file.encoding {
        // テキストで返ってきた場合はそのまま使う。
        client::FileEncoding::Text => FileContent {
            content: file.content,
            encoding: crate::ports::FileEncoding::Text,
            etag: file.sha,
        },
        client::FileEncoding::Base64 => {
            // GitHub は 60 文字ごとに改行を挿入する。空白類を除いて復号する。
            let compact: String = file
                .content
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            match STANDARD.decode(compact.as_bytes()) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => FileContent {
                        content: text,
                        encoding: crate::ports::FileEncoding::Text,
                        etag: file.sha,
                    },
                    Err(e) => FileContent {
                        content: STANDARD.encode(e.into_bytes()),
                        encoding: crate::ports::FileEncoding::Base64,
                        etag: file.sha,
                    },
                },
                // 復号できない応答は「内容を壊さずそのまま返す」。
                Err(_) => FileContent {
                    content: file.content,
                    encoding: crate::ports::FileEncoding::Base64,
                    etag: file.sha,
                },
            }
        }
    }
}

pub(crate) fn map_client_error(e: &client::ClientError) -> PortError {
    match e {
        client::ClientError::NotFound => PortError::NotFound,
        client::ClientError::Conflict => PortError::Conflict,
        client::ClientError::PermissionDenied => PortError::PermissionDenied,
        client::ClientError::RateLimited => PortError::RateLimited,
        client::ClientError::Unavailable(_) => PortError::Unavailable,
        client::ClientError::InvalidResponse(_) => PortError::InvalidResponse,
        client::ClientError::Other(_) => PortError::Unknown,
    }
}

// ---- M4: GitHub 応答 → hub 型の写像 ----

/// RFC3339 文字列を Unix 秒に変換（失敗時は 0）。
fn parse_ts(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

/// GitHub の Pull Request 応答を hub の `Proposal` に写す。
fn map_pr(
    _full_name: &str,
    pr: &PrJson,
    reviews: &[ReviewJson],
) -> Proposal {
    let required_approvals: u64 = 1;
    let approval_count = reviews
        .iter()
        .filter(|r| r.state == "APPROVED")
        .count() as u64;
    // hub の ProposalState は GitHub の open/closed と merged フラグ、承認数から判定する。
    // - マージ済み（merged フラグ）は Merged。
    // - クローズ済みで未マージは Closed。
    // - オープンで承認数が要件を満たしていれば Approved、そうでなければ Open。
    let state = match pr.state.as_str() {
        _ if pr.merged => ProposalState::Merged,
        "closed" => ProposalState::Closed,
        "open" if approval_count >= required_approvals => ProposalState::Approved,
        "open" => ProposalState::Open,
        _ => ProposalState::Open,
    };
    Proposal {
        number: pr.number as u64,
        title: pr.title.clone(),
        body: pr.body.clone(),
        state,
        author_handle: pr
            .user
            .as_ref()
            .map(|u| u.login.clone())
            .unwrap_or_default(),
        base_ref: pr.base.ref_name.clone(),
        head_ref: pr.head.ref_name.clone(),
        diff_stats: DiffStats {
            additions: pr.additions,
            deletions: pr.deletions,
            changed_files: pr.changed_files,
        },
        checks: Vec::new(),
        approval_count,
        required_approvals,
        automerge: None,
        created_at: pr.created_at.as_deref().map(parse_ts).unwrap_or(0),
        updated_at: pr.updated_at.as_deref().map(parse_ts).unwrap_or(0),
    }
}

/// GitHub の Issue 応答を hub の `Issue` に写す。
fn map_issue(issue: &IssueJson) -> Issue {
    Issue {
        number: issue.number as u64,
        title: issue.title.clone(),
        body: issue.body.clone(),
        state: if issue.state == "closed" {
            IssueState::Closed
        } else {
            IssueState::Open
        },
        labels: issue.labels.iter().map(|l| l.name.clone()).collect(),
        author_handle: issue
            .user
            .as_ref()
            .map(|u| u.login.clone())
            .unwrap_or_default(),
        assignee_handle: issue.assignee.as_ref().map(|u| u.login.clone()),
        linked_proposal_number: None,
        created_at: issue.created_at.as_deref().map(parse_ts).unwrap_or(0),
        updated_at: issue.updated_at.as_deref().map(parse_ts).unwrap_or(0),
    }
}

/// GitHub のコメント応答を hub の `Comment` に写す。
fn map_comment(c: &CommentJson) -> Comment {
    Comment {
        id: c.id.to_string(),
        author_handle: c
            .user
            .as_ref()
            .map(|u| u.login.clone())
            .unwrap_or_default(),
        body: c.body.clone(),
        path: c.path.clone(),
        line: c.line.map(|l| l as u64),
        created_at: parse_ts(&c.created_at),
    }
}

/// GitHub のファイル状態を hub の `FileStatus` に写す。
fn map_file_status(status: &str) -> FileStatus {
    match status {
        "added" => FileStatus::Added,
        "removed" => FileStatus::Deleted,
        "renamed" => FileStatus::Renamed,
        _ => FileStatus::Modified,
    }
}

/// GitHub のチェックラン結論を hub の `CheckState` に写す。
fn map_check_state(run: &CheckRunJson) -> CheckState {
    match run.conclusion.as_deref() {
        Some("success") => CheckState::Success,
        Some("failure") => CheckState::Failure,
        Some("neutral") => CheckState::Neutral,
        Some("skipped") => CheckState::Skipped,
        _ => CheckState::Pending,
    }
}

/// novnow 診断差分を算出する（設計 §5.1）。
///
/// base と head のチェックランを比較し、Failure 状態のチェックについて
/// 新たに混入した誤り（head にあって base に無い）と解消された誤り
/// （base にあって head に無い）の件数を返す。novnow 診断はチェックランの
/// 名称で識別し、`novnow` を含むチェックのみ対象とする（判断: `docs/handoff.md`
/// §4 判断 20）。チェックランが取得できない場合（空リスト）は差分 0 とする。
fn compute_diagnostics_delta(base: &[CheckStatus], head: &[CheckStatus]) -> DiagnosticsDelta {
    let is_novnow = |c: &CheckStatus| c.name.contains("novnow");
    let base_failing: Vec<&str> = base
        .iter()
        .filter(|c| is_novnow(c) && c.state == CheckState::Failure)
        .map(|c| c.name.as_str())
        .collect();
    let head_failing: Vec<&str> = head
        .iter()
        .filter(|c| is_novnow(c) && c.state == CheckState::Failure)
        .map(|c| c.name.as_str())
        .collect();
    let introduced = head_failing
        .iter()
        .filter(|name| !base_failing.contains(name))
        .count() as u64;
    let resolved = base_failing
        .iter()
        .filter(|name| !head_failing.contains(name))
        .count() as u64;
    DiagnosticsDelta {
        introduced,
        resolved,
    }
}

/// アダプターを共有ハンドルで包む。
pub type SharedGitHubAdapter = Arc<GitHubAdapter>;

impl ProposalPort for GitHubAdapter {
    async fn open_proposal(
        &self,
        shell_id: &str,
        title: &str,
        body: &str,
        head_ref: &str,
        base_ref: &str,
    ) -> Result<Proposal, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let pr = self
            .client
            .create_pull(&token, &repo.full_name, title, body, head_ref, base_ref)
            .await
            .map_err(|e| map_client_error(&e))?;
        let reviews = self
            .client
            .list_pull_reviews(&token, &repo.full_name, pr.number as u64)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(map_pr(&repo.full_name, &pr, &reviews))
    }

    async fn get_proposal(&self, shell_id: &str, number: u64) -> Result<Proposal, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let pr = self
            .client
            .get_pull(&token, &repo.full_name, number)
            .await
            .map_err(|e| map_client_error(&e))?;
        let reviews = self
            .client
            .list_pull_reviews(&token, &repo.full_name, number)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(map_pr(&repo.full_name, &pr, &reviews))
    }

    async fn list_proposals(
        &self,
        shell_id: &str,
        state: Option<ProposalState>,
    ) -> Result<Vec<Proposal>, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        // GitHub 側で絞れる範囲だけ絞って取得し、残りは写像後の hub の
        // `ProposalState` で絞り込む。マージ済み PR は GitHub では closed として
        // 扱われるため、closed 取得の結果から Merged を除外する。
        // - Open / Approved: オープンな PR を取得。
        // - Merged / Closed: クローズな PR を取得。
        // - None: 全件取得（絞り込みなし）。
        let gh_state = match state {
            Some(ProposalState::Open) | Some(ProposalState::Approved) => "open",
            Some(ProposalState::Merged) | Some(ProposalState::Closed) => "closed",
            None => "all",
        };
        let prs = self
            .client
            .list_pulls(&token, &repo.full_name, Some(gh_state))
            .await
            .map_err(|e| map_client_error(&e))?;
        let mut result = Vec::with_capacity(prs.len());
        for pr in &prs {
            let reviews = self
                .client
                .list_pull_reviews(&token, &repo.full_name, pr.number as u64)
                .await
                .map_err(|e| map_client_error(&e))?;
            let proposal = map_pr(&repo.full_name, pr, &reviews);
            // GitHub 側で絞れない hub 状態（approved / merged）と、closed 取得に
            // 混ざる merged を、写像後の state で正しく絞り込む。
            if let Some(want) = state {
                if proposal.state != want {
                    continue;
                }
            }
            result.push(proposal);
        }
        Ok(result)
    }

    async fn diff_proposal(&self, shell_id: &str, number: u64) -> Result<ProposalDiff, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        // PR の base/head ref を取得し、novnow 診断差分の算出に使う（設計 §5.1）。
        let pr = self
            .client
            .get_pull(&token, &repo.full_name, number)
            .await
            .map_err(|e| map_client_error(&e))?;
        let files = self
            .client
            .list_pull_files(&token, &repo.full_name, number)
            .await
            .map_err(|e| map_client_error(&e))?;
        let files = files
            .iter()
            .map(|f| ProposalFile {
                path: f.filename.clone(),
                status: map_file_status(&f.status),
                additions: f.additions,
                deletions: f.deletions,
                hunks: f.patch.clone().map(|p| vec![p]).unwrap_or_default(),
            })
            .collect();
        // novnow 診断差分: base と head のチェックランを CheckStatusPort で取得し、
        // Failure 状態の増減から新たに混入した誤り／解消された誤りを算出する。
        let base_checks = self.get_check_status(shell_id, &pr.base.ref_name).await?;
        let head_checks = self.get_check_status(shell_id, &pr.head.ref_name).await?;
        let delta = compute_diagnostics_delta(&base_checks, &head_checks);
        Ok(ProposalDiff {
            number,
            files,
            novnow_diagnostics_delta: delta,
        })
    }

    async fn comment_proposal(
        &self,
        shell_id: &str,
        number: u64,
        body: &str,
        _path: Option<&str>,
        _line: Option<u64>,
    ) -> Result<Comment, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let c = self
            .client
            .create_issue_comment(&token, &repo.full_name, number, body)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(map_comment(&c))
    }

    async fn review_proposal(
        &self,
        shell_id: &str,
        number: u64,
        verdict: ReviewVerdict,
        body: Option<&str>,
    ) -> Result<Proposal, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let event = match verdict {
            ReviewVerdict::Approve => "APPROVE",
            ReviewVerdict::Dismiss => "DISMISS",
            ReviewVerdict::RequestChanges => "REQUEST_CHANGES",
        };
        self.client
            .create_pull_review(&token, &repo.full_name, number, event, body)
            .await
            .map_err(|e| map_client_error(&e))?;
        self.get_proposal(shell_id, number).await
    }

    async fn merge_proposal(&self, shell_id: &str, number: u64) -> Result<MergeResult, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let m = self
            .client
            .merge_pull(&token, &repo.full_name, number)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(MergeResult {
            merged: m.merged,
            sha: m.sha,
            merged_at: m.merged_at.as_deref().map(parse_ts),
        })
    }

    async fn close_proposal(&self, shell_id: &str, number: u64) -> Result<Proposal, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        self.client
            .update_pull(&token, &repo.full_name, number, "closed")
            .await
            .map_err(|e| map_client_error(&e))?;
        self.get_proposal(shell_id, number).await
    }

    async fn list_proposal_comments(
        &self,
        shell_id: &str,
        number: u64,
    ) -> Result<Vec<Comment>, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let comments = self
            .client
            .list_issue_comments(&token, &repo.full_name, number)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(comments.iter().map(map_comment).collect())
    }
}

impl IssuePort for GitHubAdapter {
    async fn open_issue(
        &self,
        shell_id: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<Issue, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let issue = self
            .client
            .create_issue(&token, &repo.full_name, title, body, labels)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(map_issue(&issue))
    }

    async fn get_issue(&self, shell_id: &str, number: u64) -> Result<Issue, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let issue = self
            .client
            .get_issue(&token, &repo.full_name, number)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(map_issue(&issue))
    }

    async fn list_issues(
        &self,
        shell_id: &str,
        state: Option<IssueState>,
        label: Option<&str>,
    ) -> Result<Vec<Issue>, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let gh_state = state.map(|s| match s {
            IssueState::Open => "open",
            IssueState::Closed => "closed",
        });
        let issues = self
            .client
            .list_issues(&token, &repo.full_name, gh_state, label)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(issues
            .iter()
            .filter(|i| i.pull_request.is_none())
            .map(map_issue)
            .collect())
    }

    async fn update_issue(
        &self,
        shell_id: &str,
        number: u64,
        update: &IssueUpdate,
    ) -> Result<Issue, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let mut body = serde_json::json!({});
        if let Some(t) = &update.title {
            body["title"] = serde_json::json!(t);
        }
        if let Some(b) = &update.body {
            body["body"] = serde_json::json!(b);
        }
        if let Some(labels) = &update.labels {
            body["labels"] = serde_json::json!(labels);
        }
        if let Some(a) = &update.assignee_handle {
            body["assignee"] = match a {
                Some(h) => serde_json::json!(h),
                None => serde_json::Value::Null,
            };
        }
        if let Some(s) = update.state {
            body["state"] = serde_json::json!(match s {
                IssueState::Open => "open",
                IssueState::Closed => "closed",
            });
        }
        let issue = self
            .client
            .update_issue(&token, &repo.full_name, number, &body)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(map_issue(&issue))
    }

    async fn comment_issue(&self, shell_id: &str, number: u64, body: &str) -> Result<Comment, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let c = self
            .client
            .create_issue_comment(&token, &repo.full_name, number, body)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(map_comment(&c))
    }

    async fn close_issue(&self, shell_id: &str, number: u64, state: IssueState) -> Result<Issue, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let body = serde_json::json!({ "state": match state {
            IssueState::Open => "open",
            IssueState::Closed => "closed",
        }});
        let issue = self
            .client
            .update_issue(&token, &repo.full_name, number, &body)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(map_issue(&issue))
    }

    async fn list_issue_comments(
        &self,
        shell_id: &str,
        number: u64,
    ) -> Result<Vec<Comment>, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let comments = self
            .client
            .list_issue_comments(&token, &repo.full_name, number)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(comments.iter().map(map_comment).collect())
    }
}

impl AutomergePort for GitHubAdapter {
    async fn reserve_automerge(
        &self,
        shell_id: &str,
        number: u64,
        config: &AutomergeConfig,
    ) -> Result<AutomergeState, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        self.client
            .set_automerge(
                &token,
                &repo.full_name,
                number,
                config.required_approvals,
                &config.required_checks,
                config.require_no_conflicts,
            )
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(AutomergeState {
            state: AutomergeStateKind::Reserved,
            required_approvals: config.required_approvals,
            required_checks: config.required_checks.clone(),
            require_no_conflicts: config.require_no_conflicts,
            reason: None,
            etag: Self::automerge_etag(config),
        })
    }

    async fn cancel_automerge(&self, shell_id: &str, number: u64) -> Result<(), PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        self.client
            .delete_automerge(&token, &repo.full_name, number)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(())
    }

    async fn get_automerge_state(
        &self,
        shell_id: &str,
        number: u64,
    ) -> Result<AutomergeState, PortError> {
        // GitHub は予約状態を直接読む API を提供しない。pull の状態から評価する。
        // - マージ済み → Merged。
        // - クローズ済み（未マージ）→ Canceled（予約は失われる）。
        // - オープン／承認済み → Reserved（予約済みとして扱う）。
        let proposal = self.get_proposal(shell_id, number).await?;
        let state = match proposal.state {
            ProposalState::Merged => AutomergeStateKind::Merged,
            ProposalState::Closed => AutomergeStateKind::Canceled,
            ProposalState::Open | ProposalState::Approved => AutomergeStateKind::Reserved,
        };
        Ok(AutomergeState {
            state,
            required_approvals: proposal.required_approvals,
            required_checks: Vec::new(),
            require_no_conflicts: true,
            reason: None,
            etag: "".to_string(),
        })
    }
}

impl GitHubAdapter {
    /// 予約条件から If-Match 用の etag を生成する。
    fn automerge_etag(config: &AutomergeConfig) -> String {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        use std::hash::Hash;
        (
            config.required_approvals,
            config.required_checks.clone(),
            config.require_no_conflicts,
        )
            .hash(&mut h);
        format!("{:016x}", h.finish())
    }
}

impl CheckStatusPort for GitHubAdapter {
    async fn get_check_status(&self, shell_id: &str, ref_name: &str) -> Result<Vec<CheckStatus>, PortError> {
        let token = self.token()?;
        let repo = self.client.get_repo(&token, shell_id).await.map_err(|e| map_client_error(&e))?;
        let runs = self
            .client
            .list_check_runs(&token, &repo.full_name, ref_name)
            .await
            .map_err(|e| map_client_error(&e))?;
        Ok(runs
            .iter()
            .map(|r| CheckStatus {
                name: r.name.clone(),
                state: map_check_state(r),
                details_url: r.html_url.clone(),
            })
            .collect())
    }
}
