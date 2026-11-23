//! セッション管理。
//!
//! 設計書 `REF:docs/designs/api.md` §3.2 に準拠する。セッションは不透明乱数
//! （256bit）+ サーバ照合（JWT にしない）とし、インメモリで保持する。
//! アクセストークンは 1 時間、更新トークンは 30 日・ローテーション・
//! 旧トークンの再利用検知（再利用時は当該利用者の全セッションを失効）とする。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use rand::Rng;
use serde::Serialize;

use crate::config::Config;

/// セッションの状態。
#[derive(Debug, Clone)]
pub struct Session {
    /// 不透明なセッション識別子（クライアントに返すアクセストークン）。
    pub session_id: String,
    /// 更新トークン（ローテーション対象）。
    pub refresh_token: String,
    /// アクセストークンの発行時の有効秒数（設定値）。
    pub expires_in: i64,
    /// アクセストークンの有効期限。
    pub expires_at: Instant,
    /// セッションの有効期限（更新トークンの期限）。
    pub refresh_expires_at: Instant,
    /// 失効済みかどうか。
    pub revoked: bool,
    /// このセッションを作った利用者の ID。
    pub user_id: String,
}

/// クライアントへ返すトークン対（契約の `TokenPair` スキーマに準拠）。
#[derive(Debug, Serialize)]
pub struct TokenPair {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: i64,
}

/// セッションの状態を示すエラー。
#[derive(Debug)]
pub enum SessionError {
    /// セッションが期限切れ・失効。
    Expired,
    /// 更新トークンの再利用検知。
    Reused,
}

/// インメモリセッションストア。
#[derive(Debug)]
pub struct SessionStore {
    sessions: RwLock<HashMap<String, Session>>,
    /// 使用済みの更新トークン → 利用者の ID（再利用検知用）。
    used_refresh_tokens: RwLock<std::collections::HashMap<String, String>>,
    /// 一時コード → session_id（callback で発行、refresh で消費）。
    temp_codes: RwLock<HashMap<String, String>>,
    config: Config,
}

impl SessionStore {
    pub fn new(config: Config) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            used_refresh_tokens: RwLock::new(std::collections::HashMap::new()),
            temp_codes: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// セッションに紐付ける使い捨ての一時コードを発行する。
    ///
    /// 設計 §3・`REF:api/openapi.yaml` の 302 記述に準拠。アクセストークン
    /// を URL に残さないため、callback はこのコードをクライアントへ渡し、
    /// SPA が直後に `refresh` でセッションを得る。
    pub fn issue_temp_code(&self, session_id: &str) -> String {
        let mut rng = rand::thread_rng();
        let code = random_token(&mut rng);
        self.temp_codes
            .write()
            .unwrap()
            .insert(code.clone(), session_id.to_string());
        code
    }

    /// 一時コードを照合し、対応するセッションを返す（使い捨て）。
    ///
    /// コードが無効・既に使用済み・対応セッションが失効の場合は `None`。
    pub fn exchange_temp_code(&self, code: &str) -> Option<Session> {
        let session_id = self.temp_codes.write().unwrap().remove(code)?;
        self.verify_access(&session_id)
    }

    /// 新しいセッションを作成する。
    pub fn create(&self, user_id: &str) -> Session {
        let mut rng = rand::thread_rng();
        let session_id = random_token(&mut rng);
        let refresh_token = random_token(&mut rng);
        let now = Instant::now();
        let session = Session {
            session_id: session_id.clone(),
            refresh_token: refresh_token.clone(),
            expires_in: self.config.access_token_ttl_secs,
            expires_at: now + Duration::from_secs(self.config.access_token_ttl_secs as u64),
            refresh_expires_at: now
                + Duration::from_secs(self.config.refresh_token_ttl_secs as u64),
            revoked: false,
            user_id: user_id.to_string(),
        };
        self.sessions
            .write()
            .unwrap()
            .insert(session_id, session.clone());
        session
    }

    /// アクセストークンを照合する。有効ならセッションを返す。
    ///
    /// 失効済み、アクセストークンの有効期限（`expires_at`）超過、更新
    /// トークンの有効期限（`refresh_expires_at`）超過のいずれも不可。
    pub fn verify_access(&self, session_id: &str) -> Option<Session> {
        let sessions = self.sessions.read().unwrap();
        let session = sessions.get(session_id)?;
        let now = Instant::now();
        if session.revoked
            || session.expires_at <= now
            || session.refresh_expires_at <= now
        {
            return None;
        }
        Some(session.clone())
    }

    /// アクセストークンの残り有効秒数（`expires_at` から算出）。最小 0。
    pub fn remaining_access_secs(&self, session: &Session) -> i64 {
        let now = Instant::now();
        if session.expires_at <= now {
            return 0;
        }
        i64::try_from(session.expires_at.duration_since(now).as_secs()).unwrap_or(0)
    }

    /// 更新トークンをローテーションする。
    ///
    /// 再利用検知: 既に使用済みの更新トークンが再提示された場合は当該利用者の
    /// 全セッションを失効させ `Reused` を返す。使用済みトークンは
    /// `used_refresh_tokens` に残すため、旧トークンの再提示は `Expired`
    /// ではなく `Reused` として検知される。
    pub fn rotate(&self, refresh_token: &str) -> Result<Session, SessionError> {
        let mut sessions = self.sessions.write().unwrap();
        // 使用済みトークンの再提示を先に検知する。
        let reused_user = self
            .used_refresh_tokens
            .read()
            .unwrap()
            .get(refresh_token)
            .cloned();
        if let Some(uid) = reused_user {
            // 当該利用者の全セッションを失効させる。
            for s in sessions.values_mut() {
                if s.user_id == uid {
                    s.revoked = true;
                }
            }
            return Err(SessionError::Reused);
        }

        let session = sessions
            .values()
            .find(|s| s.refresh_token == refresh_token)
            .cloned()
            .ok_or(SessionError::Expired)?;
        if session.revoked || session.refresh_expires_at <= Instant::now() {
            return Err(SessionError::Expired);
        }

        // ローテーション。使用済みトークンを記録し、次回の再提示を検知する。
        let mut rng = rand::thread_rng();
        let new_session_id = random_token(&mut rng);
        let new_refresh_token = random_token(&mut rng);
        sessions.remove(&session.session_id);
        self.used_refresh_tokens
            .write()
            .unwrap()
            .insert(session.refresh_token.clone(), session.user_id.clone());
        let now = Instant::now();
        let new_session = Session {
            session_id: new_session_id.clone(),
            refresh_token: new_refresh_token.clone(),
            expires_in: self.config.access_token_ttl_secs,
            expires_at: now + Duration::from_secs(self.config.access_token_ttl_secs as u64),
            refresh_expires_at: now
                + Duration::from_secs(self.config.refresh_token_ttl_secs as u64),
            revoked: false,
            user_id: session.user_id.clone(),
        };
        sessions.insert(new_session_id, new_session.clone());
        Ok(new_session)
    }

    /// セッションを失効させる。
    pub fn revoke(&self, session_id: &str) {
        if let Some(session) = self.sessions.write().unwrap().get_mut(session_id) {
            session.revoked = true;
        }
    }

    /// 利用者の全セッションを失効させる。
    pub fn revoke_all_for_user(&self, user_id: &str) {
        for session in self.sessions.write().unwrap().values_mut() {
            if session.user_id == user_id {
                session.revoked = true;
            }
        }
    }
}

/// 256bit の不透明乱数トークン（URL 安全な Base64）。
fn random_token(rng: &mut impl Rng) -> String {
    let bytes: [u8; 32] = rng.gen();
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(bytes)
}

/// 共有ハンドル。
pub type SharedSessionStore = Arc<SessionStore>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn store() -> SessionStore {
        SessionStore::new(Config::from_env())
    }

    #[test]
    fn create_and_verify_access() {
        let s = store();
        let session = s.create("user-1");
        assert!(!session.session_id.is_empty());
        assert!(!session.refresh_token.is_empty());
        let verified = s.verify_access(&session.session_id).expect("有効なアクセストークン");
        assert_eq!(verified.user_id, "user-1");
    }

    #[test]
    fn verify_access_rejects_unknown() {
        let s = store();
        assert!(s.verify_access("no-such-token").is_none());
    }

    #[test]
    fn rotate_returns_new_pair() {
        let s = store();
        let session = s.create("user-1");
        let rotated = s.rotate(&session.refresh_token).expect("ローテーション成功");
        assert_ne!(rotated.refresh_token, session.refresh_token);
        // ローテーション後、旧 refresh_token は使用済みとして記録される。
        // 再提示は Expired（見つからない）ではなく、再利用検知で Reused。
        assert!(matches!(
            s.rotate(&session.refresh_token),
            Err(SessionError::Reused)
        ));
    }

    #[test]
    fn rotate_reused_token_revokes_all_sessions_for_user() {
        let s = store();
        let session = s.create("user-1");
        let rotated = s.rotate(&session.refresh_token).expect("ローテーション成功");
        // 旧トークンの再利用を検知 → 当該利用者の全セッション失効。
        assert!(matches!(
            s.rotate(&session.refresh_token),
            Err(SessionError::Reused)
        ));
        // 回転で得た新しいセッションも失効している。
        assert!(s.verify_access(&rotated.session_id).is_none());
    }

    #[test]
    fn expires_at_gates_access_token() {
        let s = store();
        let session = s.create("user-1");
        // expires_at を過去に設定して期限切れを再現。
        let mut expired = session.clone();
        expired.expires_at = Instant::now() - Duration::from_secs(1);
        {
            let mut sessions = s.sessions.write().unwrap();
            sessions.insert(expired.session_id.clone(), expired.clone());
        }
        assert!(s.verify_access(&session.session_id).is_none());
        // 残り秒数は 0。
        assert_eq!(s.remaining_access_secs(&expired), 0);
    }

    #[test]
    fn refresh_expiry_gates_access_token() {
        let s = store();
        let mut session = s.create("user-1");
        session.refresh_expires_at = Instant::now() - Duration::from_secs(1);
        {
            let mut sessions = s.sessions.write().unwrap();
            sessions.insert(session.session_id.clone(), session.clone());
        }
        assert!(s.verify_access(&session.session_id).is_none());
    }

    #[test]
    fn temp_code_exchange_yields_session() {
        let s = store();
        let session = s.create("user-1");
        let code = s.issue_temp_code(&session.session_id);
        let exchanged = s
            .exchange_temp_code(&code)
            .expect("一時コードでセッション取得");
        assert_eq!(exchanged.session_id, session.session_id);
        // 使い捨て: 2 回目の照合は失敗。
        assert!(s.exchange_temp_code(&code).is_none());
    }

    #[test]
    fn rotate_unknown_token_is_expired() {
        let s = store();
        assert!(matches!(s.rotate("unknown"), Err(SessionError::Expired)));
    }

    #[test]
    fn revoke_invalidates_access() {
        let s = store();
        let session = s.create("user-1");
        s.revoke(&session.session_id);
        assert!(s.verify_access(&session.session_id).is_none());
    }
}
