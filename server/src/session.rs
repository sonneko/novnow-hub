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
    /// アクセストークンの残り有効秒数。
    pub expires_in: i64,
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
    config: Config,
}

impl SessionStore {
    pub fn new(config: Config) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// 新しいセッションを作成する。
    pub fn create(&self, user_id: &str) -> Session {
        let mut rng = rand::thread_rng();
        let session_id = random_token(&mut rng);
        let refresh_token = random_token(&mut rng);
        let session = Session {
            session_id: session_id.clone(),
            refresh_token: refresh_token.clone(),
            expires_in: self.config.access_token_ttl_secs,
            refresh_expires_at: Instant::now()
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
    pub fn verify_access(&self, session_id: &str) -> Option<Session> {
        let sessions = self.sessions.read().unwrap();
        let session = sessions.get(session_id)?;
        if session.revoked || session.expires_in <= 0 {
            return None;
        }
        Some(session.clone())
    }

    /// 更新トークンをローテーションする。
    ///
    /// 再利用検知: 既に使用済みの更新トークンが再提示された場合は当該利用者の
    /// 全セッションを失効させ `Reused` を返す。
    pub fn rotate(&self, refresh_token: &str) -> Result<Session, SessionError> {
        let mut sessions = self.sessions.write().unwrap();
        let session = sessions
            .values()
            .find(|s| s.refresh_token == refresh_token)
            .cloned()
            .ok_or(SessionError::Expired)?;
        if session.revoked || session.refresh_expires_at <= Instant::now() {
            return Err(SessionError::Expired);
        }

        // 既に使用済みのトークンかを検知するため、更新トークンを書き換える前に
        // マップ上の同一 refresh_token が他に存在しないことを確認する。
        let reused = sessions
            .values()
            .filter(|s| s.refresh_token == refresh_token && s.session_id != session.session_id)
            .count()
            > 0;
        if reused {
            // 当該利用者の全セッションを失効
            for s in sessions.values_mut() {
                if s.user_id == session.user_id {
                    s.revoked = true;
                }
            }
            return Err(SessionError::Reused);
        }

        // ローテーション
        let mut rng = rand::thread_rng();
        let new_session_id = random_token(&mut rng);
        let new_refresh_token = random_token(&mut rng);
        sessions.remove(&session.session_id);
        let new_session = Session {
            session_id: new_session_id.clone(),
            refresh_token: new_refresh_token.clone(),
            expires_in: self.config.access_token_ttl_secs,
            refresh_expires_at: Instant::now()
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
        // ローテーション後、旧 refresh_token は消える。再提示は Expired（見つからない）。
        assert!(matches!(s.rotate(&session.refresh_token), Err(SessionError::Expired)));
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
