//! アプリケーション状態（Axum の共有状態）。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::adapters::github::SharedGitHubAdapter;
use crate::config::Config;
use crate::routes::auth::StateStore;
use crate::session::SharedSessionStore;

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
}
