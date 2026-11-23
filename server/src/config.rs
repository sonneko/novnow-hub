//! 環境変数からの設定読み込み。
//!
//! 認証まわりは環境変数からの設定読み込み＋未設定時の明確なエラーで実装する。
//! 実際の資格情報はコードやドキュメントに書かない（本ファイルにも値は含まない）。

use std::env;

/// バックエンドの設定。
#[derive(Debug, Clone)]
pub struct Config {
    /// 契約の版（`api/openapi.yaml` の `info.version` と一致させる）。
    pub contract_version: String,
    /// 最小クライアント版。
    pub minimum_client_version: String,
    /// GitHub OAuth のクライアント ID（未設定なら認証フローはエラー）。
    pub github_oauth_client_id: Option<String>,
    /// GitHub OAuth のクライアントシークレット（未設定なら認証フローはエラー）。
    pub github_oauth_client_secret: Option<String>,
    /// OAuth コールバックの URL（GitHub 側の登録と一致させる）。
    pub github_oauth_redirect_uri: Option<String>,
    /// GitHub の API ベース URL（既定 `https://api.github.com`）。
    pub github_api_base: String,
    /// GitHub の認証ベース URL（既定 `https://github.com`）。
    pub github_auth_base: String,
    /// セッションのアクセストークン有効秒数（既定 3600 = 1 時間）。
    pub access_token_ttl_secs: i64,
    /// セッションの更新トークン有効秒数（既定 30 日）。
    pub refresh_token_ttl_secs: i64,
    /// addr で待受するアドレス（既定 `127.0.0.1`）。
    pub bind_addr: String,
    /// 読み取りリクエストのタイムアウト秒数（既定 5。設計 §7 の共通規約）。
    pub read_timeout_secs: u64,
    /// 書き込みリクエストのタイムアウト秒数（既定 30。設計 §7 の共通規約）。
    pub write_timeout_secs: u64,
    /// コミット操作のタイムアウト秒数（既定 120。設計 §7 の共通規約）。
    pub commit_timeout_secs: u64,
    /// 待受ポート（既定 8080）。
    pub port: u16,
}

impl Config {
    /// 環境変数から読み込む。未設定項目は既定値または None。
    pub fn from_env() -> Self {
        let get = |key: &str| env::var(key).ok().filter(|v| !v.is_empty());
        Config {
            contract_version: get("HUB_CONTRACT_VERSION").unwrap_or_else(|| "1.0.0".to_string()),
            minimum_client_version: get("HUB_MINIMUM_CLIENT_VERSION")
                .unwrap_or_else(|| "1.0.0".to_string()),
            github_oauth_client_id: get("HUB_GITHUB_OAUTH_CLIENT_ID"),
            github_oauth_client_secret: get("HUB_GITHUB_OAUTH_CLIENT_SECRET"),
            github_oauth_redirect_uri: get("HUB_GITHUB_OAUTH_REDIRECT_URI"),
            github_api_base: get("HUB_GITHUB_API_BASE")
                .unwrap_or_else(|| "https://api.github.com".to_string()),
            github_auth_base: get("HUB_GITHUB_AUTH_BASE")
                .unwrap_or_else(|| "https://github.com".to_string()),
            access_token_ttl_secs: get("HUB_ACCESS_TOKEN_TTL_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
            refresh_token_ttl_secs: get("HUB_REFRESH_TOKEN_TTL_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30 * 24 * 3600),
            bind_addr: get("HUB_BIND_ADDR").unwrap_or_else(|| "127.0.0.1".to_string()),
            read_timeout_secs: get("HUB_READ_TIMEOUT_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            write_timeout_secs: get("HUB_WRITE_TIMEOUT_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            commit_timeout_secs: get("HUB_COMMIT_TIMEOUT_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(120),
            port: get("HUB_PORT").and_then(|v| v.parse().ok()).unwrap_or(8080),
        }
    }

    /// GitHub OAuth の設定が揃っているか（未設定時の明確なエラー用）。
    pub fn github_oauth_ready(&self) -> Result<(), String> {
        let mut missing = Vec::new();
        if self.github_oauth_client_id.is_none() {
            missing.push("HUB_GITHUB_OAUTH_CLIENT_ID");
        }
        if self.github_oauth_client_secret.is_none() {
            missing.push("HUB_GITHUB_OAUTH_CLIENT_SECRET");
        }
        if self.github_oauth_redirect_uri.is_none() {
            missing.push("HUB_GITHUB_OAUTH_REDIRECT_URI");
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!("GitHub OAuth の設定が未設定です: {}", missing.join(", ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_uses_defaults() {
        // 環境変数が無ければ既定値が使われる。
        let c = Config::from_env();
        assert_eq!(c.contract_version, "1.0.0");
        assert_eq!(c.github_api_base, "https://api.github.com");
        assert_eq!(c.port, 8080);
        assert_eq!(c.read_timeout_secs, 5);
        assert_eq!(c.write_timeout_secs, 30);
        assert_eq!(c.commit_timeout_secs, 120);
    }

    #[test]
    fn github_oauth_ready_reports_missing() {
        let c = Config::from_env();
        // 沙箱には OAuth 環境変数が無い前提。未設定ならエラー内容に項目名が含まれる。
        let err = c.github_oauth_ready().expect_err("未設定ならエラー");
        assert!(err.contains("HUB_GITHUB_OAUTH_CLIENT_ID"));
    }
}
