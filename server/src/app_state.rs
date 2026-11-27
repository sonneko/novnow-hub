//! アプリケーション状態（Axum の共有状態）。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::adapters::github::SharedGitHubAdapter;
use crate::config::Config;
use crate::derivatives::SharedDerivativeStore;
use crate::jobs::SharedJobPort;
use crate::registry::SharedProjectRegistry;
use crate::routes::auth::StateStore;
use crate::session::SharedSessionStore;
use crate::notifications::SharedNotificationStore;
use crate::verification::SharedVerificationService;
use crate::webhook::{SharedRebuild, SharedWebhookStore};

/// 共有する派生アダプター（`DerivationPort` 実装）。
pub type SharedDerivation = std::sync::Arc<dyn crate::ports::DerivationPort>;

/// 全ルーターで共有する状態。
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub sessions: SharedSessionStore,
    pub github_adapter: SharedGitHubAdapter,
    pub state_store: Arc<StateStore>,
    /// state → code_verifier の一時保存（callback まで）。
    pub verifier_store: Arc<RwLock<HashMap<String, String>>>,
    /// state → クライアント側リダイレクト先の一時保存。
    pub redirect_store: Arc<RwLock<HashMap<String, String>>>,
    /// インメモリのプロジェクト登録簿（判断: `REF:docs/handoff.md` §4 判断 2）。
    pub projects: SharedProjectRegistry,
    /// 通知の hub 管理ストア（設計 §6）。
    pub notifications: SharedNotificationStore,
    /// Webhook 受信の再生成ジョブストア（設計 §7.1）。
    pub webhook: SharedWebhookStore,
    /// 公開ページ再生成ポート（設計 §7.1、既定はロギング実装）。
    pub rebuild: SharedRebuild,
    /// 派生作品の hub 管理ストア（設計 §2.2）。
    pub derivatives: SharedDerivativeStore,
    /// 派生アダプター（GitHub Fork 連携、設計 §2.2）。
    pub derivation: SharedDerivation,
    /// novnow 診断・エクスポートの非同期ジョブ基盤（設計 §4.1）。
    pub jobs: SharedJobPort,
    /// 公開前検証（novnow 点検）の同期検証サービス（設計 §6.1）。
    pub verification: SharedVerificationService,
}
