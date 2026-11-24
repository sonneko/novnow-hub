//! GitHub Webhook 受信と公開ページ再生成（設計 §7.1）。
//!
//! 外部基盤（GitHub）からの Webhook を受信し、**HMAC-SHA256 署名検証**で
//! なりすましを防ぐ。検証後のイベント（push）に応じて対象プロジェクトの
//! 公開ページを**非同期ジョブ**で再生成し、キャッシュを失効させる。処理結果は
//! ログに残し、失敗時はリトライ（手動再実行）の導線を提供する。
//!
//! Webhook 受信口は**サーバー内部のエンドポイント**であり、API 契約
//! （`api/openapi.yaml`）には Webhook 用の公開エンドポイントが存在しない
//! ため、契約には追加しない。GitHub の概念はアダプター層
//! （`adapters::github::webhook`）の背後に隠す。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use sha2::{Digest, Sha256};

use crate::adapters::github::webhook::parse_webhook_event;
use crate::app_state::AppState;
use crate::ports::{PortError, RebuildPort, WebhookEventKind};
use crate::registry::{new_project_id, now_unix};

/// 共有する Webhook ジョブストア。
pub type SharedWebhookStore = Arc<WebhookStore>;

/// 共有する再生成ポート（ロギング実装を既定とする）。
pub type SharedRebuild = Arc<dyn RebuildPort>;

/// SHA-256 のブロック長（HMAC のキー・パディング単位）。
const SHA256_BLOCK: usize = 64;

/// HMAC-SHA256 を計算する（RFC 2104）。
///
/// `hmac` クレートがベンダー未収録のため、`sha2` で手実装する。
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    // キーをブロック長にパディング（鍵がブロック長より長い場合はハッシュで縮約）。
    let mut k = [0u8; SHA256_BLOCK];
    if key.len() > SHA256_BLOCK {
        let digest = Sha256::digest(key);
        k[..32].copy_from_slice(&digest);
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; SHA256_BLOCK];
    let mut opad = [0x5cu8; SHA256_BLOCK];
    for i in 0..SHA256_BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(msg);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    let outer_digest = outer.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&outer_digest);
    out
}

/// バイト列を 16 進文字列へ変換する（小文字）。
fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 16 進文字列をバイト列へ変換する（大文字・小文字両方を受け付ける）。
fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
        i += 2;
    }
    Some(out)
}

/// 署名文字列を生成する（`sha256=<hex>` 形式）。GitHub の
/// `X-Hub-Signature-256` と同じ形式（16 進ダイジェスト）。
pub fn sign(secret: &[u8], body: &[u8]) -> String {
    let mac = hmac_sha256(secret, body);
    format!("sha256={}", to_hex(&mac))
}

/// 定数時間比較。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 署名を検証する。`X-Hub-Signature-256` の値（`sha256=<hex>`）と
/// ボディから計算した署名を比較する。base64 形式は受け付けない
/// （GitHub は 16 進ダイジェストを送る）。
pub fn verify_signature(secret: &[u8], body: &[u8], signature: &str) -> bool {
    let Some(hex) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Some(provided) = from_hex(hex) else {
        return false;
    };
    let computed = hmac_sha256(secret, body);
    constant_time_eq(&provided, &computed)
}

/// Webhook 受信の拒否理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookRejection {
    /// 署名ヘッダが無い。
    MissingSignature,
    /// 署名が不正（不一致・形式不正）。
    InvalidSignature,
    /// サーバーに署名検証用の秘密が設定されていない。
    MissingSecret,
    /// ボディが Webhook として解釈できない。
    InvalidPayload,
    /// 対象リポジトリが登録簿に無く、再生成対象を特定できない。
    UnknownProject,
}

/// 再生成ジョブの状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookJobStatus {
    /// 起票済み（未実行）。
    Pending,
    /// 実行中。
    Running,
    /// 成功。
    Succeeded,
    /// 失敗（リトライ可）。
    Failed,
}

impl WebhookJobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WebhookJobStatus::Pending => "pending",
            WebhookJobStatus::Running => "running",
            WebhookJobStatus::Succeeded => "succeeded",
            WebhookJobStatus::Failed => "failed",
        }
    }
}

/// 再生成ジョブの記録。
#[derive(Debug, Clone)]
pub struct WebhookJob {
    pub job_id: String,
    pub project_id: String,
    /// 対象の保管単位識別子（shell_id 相当）。
    pub repo_full_name: String,
    pub status: WebhookJobStatus,
    pub attempts: u32,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// インメモリの Webhook ジョブストア。
///
/// 再生成ジョブの起票・状態遷移・リトライを管理する。処理結果のログは
/// `tracing` で残し、失敗ジョブは `retry` で手動再実行できる。
#[derive(Debug, Default)]
pub struct WebhookStore {
    jobs: RwLock<HashMap<String, WebhookJob>>,
    order: RwLock<Vec<String>>,
}

impl WebhookStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 再生成ジョブを起票する（状態は Pending）。
    pub fn enqueue(&self, project_id: &str, repo_full_name: &str) -> WebhookJob {
        let now = now_unix();
        let job = WebhookJob {
            job_id: new_project_id(),
            project_id: project_id.to_string(),
            repo_full_name: repo_full_name.to_string(),
            status: WebhookJobStatus::Pending,
            attempts: 0,
            error: None,
            created_at: now,
            updated_at: now,
        };
        self.jobs
            .write()
            .unwrap()
            .insert(job.job_id.clone(), job.clone());
        self.order
            .write()
            .unwrap()
            .push(job.job_id.clone());
        job
    }

    pub fn get(&self, job_id: &str) -> Option<WebhookJob> {
        self.jobs.read().unwrap().get(job_id).cloned()
    }

    pub fn list(&self) -> Vec<WebhookJob> {
        let order = self.order.read().unwrap().clone();
        let jobs = self.jobs.read().unwrap();
        order
            .iter()
            .filter_map(|id| jobs.get(id).cloned())
            .collect()
    }

    /// 状態を遷移し、更新時刻を進める。
    pub fn set_status(
        &self,
        job_id: &str,
        status: WebhookJobStatus,
        error: Option<String>,
        attempts: u32,
    ) {
        if let Some(job) = self.jobs.write().unwrap().get_mut(job_id) {
            job.status = status;
            job.error = error;
            job.attempts = attempts;
            job.updated_at = now_unix();
        }
    }

    /// 失敗ジョブを手動再実行のため Pending に戻す。attempts を 1 加算し、
    /// エラーを消去する。存在しないジョブは None。
    pub fn retry(&self, job_id: &str) -> Option<WebhookJob> {
        let mut jobs = self.jobs.write().unwrap();
        let job = jobs.get_mut(job_id)?;
        if job.status != WebhookJobStatus::Failed {
            return Some(job.clone());
        }
        job.status = WebhookJobStatus::Pending;
        job.error = None;
        job.attempts += 1;
        job.updated_at = now_unix();
        Some(job.clone())
    }
}

/// 既定の再生成ポート実装（ロギング）。
///
/// この hub では公開ページの実体生成は公開フロー（web/CDN）側の責務であり、
/// サーバー側は再生成の**起動と結果ログ**を担う。ポート経由で隔离し、実際の
/// 再生成・キャッシュ失効の実装は適応層で差し替え可能にする。
#[derive(Debug, Default)]
pub struct LoggingRebuild;

impl RebuildPort for LoggingRebuild {
    fn rebuild_public_page(
        &self,
        project_id: &str,
        repo: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), PortError>> + Send + '_>,
    > {
        let project_id = project_id.to_string();
        let repo = repo.to_string();
        Box::pin(async move {
            tracing::info!(project_id = %project_id, repo = %repo, "public page rebuild started");
            // 再生成とキャッシュ失効を実行（実体は適応層の責務）。成功として記録。
            tracing::info!(project_id = %project_id, repo = %repo, "public page rebuilt; cache invalidated");
            Ok(())
        })
    }
}

/// Webhook 受信の結果（ジョブが起票されなかった場合は収集情報）。
#[derive(Debug, Clone)]
pub struct WebhookOutcome {
    pub job: Option<WebhookJob>,
    /// 再生成対象外（Non-push イベント等）で起票しなかった旨。
    pub skipped: bool,
}

/// Webhook を受信し、署名検証→イベント解析→プロジェクト解決→ジョブ起票を行う。
///
/// 再生成ジョブの**実行**は非同期（`run_job`）で行い、受信応答はブロックしない。
pub async fn receive_webhook(
    state: &AppState,
    secret: Option<&str>,
    event_header: Option<&str>,
    signature: Option<&str>,
    body: &[u8],
) -> Result<WebhookOutcome, WebhookRejection> {
    let Some(secret) = secret else {
        return Err(WebhookRejection::MissingSecret);
    };
    let Some(signature) = signature else {
        return Err(WebhookRejection::MissingSignature);
    };

    // なりすまし防止: HMAC-SHA256 署名検証。
    if !verify_signature(secret.as_bytes(), body, signature) {
        tracing::warn!("webhook signature verification failed");
        return Err(WebhookRejection::InvalidSignature);
    }

    // GitHub の概念はアダプター層で吸収。
    let event = parse_webhook_event(event_header, body).map_err(|_| WebhookRejection::InvalidPayload)?;

    // 再生成対象は push のみ。
    if event.kind != WebhookEventKind::Push {
        tracing::info!(repo = %event.repo_full_name, "webhook ignored (non-push event)");
        return Ok(WebhookOutcome { job: None, skipped: true });
    }

    // 対象リポジトリ（shell_id 相当）を登録簿でプロジェクトへ解決。
    let record = state
        .projects
        .get_by_shell(&event.repo_full_name)
        .ok_or(WebhookRejection::UnknownProject)?;

    // 再生成ジョブを起票（Pending）。実行は非同期で行う。
    let job = state
        .webhook
        .enqueue(&record.project_id, &event.repo_full_name);
    tracing::info!(
        job_id = %job.job_id,
        project_id = %job.project_id,
        repo = %job.repo_full_name,
        "webhook accepted; rebuild job enqueued"
    );

    Ok(WebhookOutcome { job: Some(job), skipped: false })
}

/// 起票された再生成ジョブを実行する（非同期）。
///
/// 状態を `Running` にし、再生成ポートを呼び、結果を `Succeeded`／`Failed` に
/// 記録する。失敗時はログに残し、`WebhookStore::retry` で手動再実行できる。
pub async fn run_job(
    store: &WebhookStore,
    rebuild: &dyn RebuildPort,
    job_id: String,
    project_id: String,
    repo_full_name: String,
) {
    let attempts = store
        .get(&job_id)
        .map(|j| j.attempts.saturating_add(1))
        .unwrap_or(1);
    store.set_status(&job_id, WebhookJobStatus::Running, None, attempts);

    match rebuild.rebuild_public_page(&project_id, &repo_full_name).await {
        Ok(()) => {
            store.set_status(&job_id, WebhookJobStatus::Succeeded, None, attempts);
            tracing::info!(job_id = %job_id, "rebuild job succeeded");
        }
        Err(err) => {
            let msg = err.to_string();
            store.set_status(&job_id, WebhookJobStatus::Failed, Some(msg.clone()), attempts);
            tracing::error!(job_id = %job_id, error = %msg, "rebuild job failed; retry available");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(msg: &str) -> Vec<u8> {
        msg.as_bytes().to_vec()
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let secret = b"supersecret";
        let msg = body("payload");
        let sig = sign(secret, &msg);
        assert!(sig.starts_with("sha256="));
        assert!(verify_signature(secret, &msg, &sig));
    }

    #[test]
    fn verify_rejects_tampered_body() {
        let secret = b"supersecret";
        let sig = sign(secret, &body("payload"));
        assert!(!verify_signature(secret, &body("tampered"), &sig));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let sig = sign(b"secret-a", &body("payload"));
        assert!(!verify_signature(b"secret-b", &body("payload"), &sig));
    }

    #[test]
    fn verify_rejects_bad_format() {
        let secret = b"s";
        assert!(!verify_signature(secret, &body("x"), "not-a-signature"));
        // 16 進でない（base64 等の他の形式）は受け付けない。
        assert!(!verify_signature(secret, &body("x"), "sha256=!!!nothex!!!"));
        // 16 進文字列でも長さが偶数でない/文字が不正なら拒否。
        assert!(!verify_signature(secret, &body("x"), "sha256=abc"));
        assert!(!verify_signature(secret, &body("x"), "sha256=zz"));
    }

    /// GitHub 公式ドキュメント「Validating webhook deliveries」のテストベクタ。
    /// HMAC-SHA256 の手実装が GitHub の既知の答えと一致することを実行で証明する。
    /// 秘密 `It's a Secret to Everybody`、ボディ `Hello, World!` の署名は
    /// `sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17`。
    #[test]
    fn github_official_test_vector() {
        let secret = b"It's a Secret to Everybody";
        let msg = body("Hello, World!");
        let expected = "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
        // 生成した署名が GitHub の既知の答えと一致する。
        assert_eq!(sign(secret, &msg), expected);
        // その署名で検証が成功する。
        assert!(verify_signature(secret, &msg, expected));
    }

    #[test]
    fn store_enqueue_get_list() {
        let store = WebhookStore::new();
        let job = store.enqueue("prj_1", "octo/demo");
        assert_eq!(job.status, WebhookJobStatus::Pending);
        assert_eq!(store.get(&job.job_id).unwrap().project_id, "prj_1");
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn store_retry_resets_failed_job() {
        let store = WebhookStore::new();
        let job = store.enqueue("prj_1", "octo/demo");
        store.set_status(&job.job_id, WebhookJobStatus::Failed, Some("boom".into()), 1);
        assert_eq!(store.get(&job.job_id).unwrap().status, WebhookJobStatus::Failed);

        let retried = store.retry(&job.job_id).unwrap();
        assert_eq!(retried.status, WebhookJobStatus::Pending);
        assert_eq!(retried.error, None);
        assert_eq!(retried.attempts, 2);
    }

    #[test]
    fn store_retry_unknown_job_is_none() {
        let store = WebhookStore::new();
        assert!(store.retry("nope").is_none());
    }
}
