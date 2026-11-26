//! novnow-hub デスクトップアプリ（Tauri v2）。
//!
//! Web アプリ（Astro + Svelte）と同一のコードベースを WebView で表示し、
//! セッション保管（secure storage）とカスタムスキームでのセットアップ URL
//! 受信を担う。GitHub の概念はアダプター層の背後に隠し、利用者体験には漏らさない。

use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use keyring::Entry;
use tauri::Manager;

/// OS キーチェーン（macOS Keychain / Windows Credential Manager / Linux
/// Secret Service）にセッション情報を保管するバックエンド。
///
/// 要件は「Tauri secure storage（OS のキーチェーン）へのセッション保管・
/// 平文露出禁止」（仕様 `REF:docs/specifications/m6.md` §6.1、設計
/// `REF:docs/designs/m6.md` §4.1／§6.1）。
///
/// M6 では IOTA Stronghold と keyring を「コンパイル失敗」として排除し平文
/// ファイルに切り替えたが、当時の失敗は実際には CLI 引数エラー（`--bundle`）と
/// `tauri::State` の lifetime 欠落（E0726）であり、依存クレートのコンパイル
/// 可否は未検証だった（判断 15 追記）。本次（M7 セキュリティレビュー）で
/// `keyring` v3 の API をソースで確定し、OS キーチェーンへの保管に再挑戦する。
///
/// OS キーチェーンが使えないプラットフォーム（例: Linux で Secret Service
/// デーモンが未起動の環境）では、具体的な失敗理由を `docs/handoff.md` の判断
/// 記録に残し、**暗号化ファイル**（AES-256-GCM、鍵はアプリデータディレクトリ
/// 由来の導出値）へフォールバックする。**平文は残さない**。
const KEYRING_SERVICE: &str = "novnow-hub";
/// 暗号化ファイルの鍵導出に用いるソルト（定数）。
const FILE_KEY_SALT: &str = "novnow-hub:session-key:v1";

/// セッションキーと保管バックエンドを紐付ける。中身は永続的な設定のみ。
#[derive(Default)]
struct SecureStorage {
    dir: std::sync::Mutex<Option<PathBuf>>,
}

impl SecureStorage {
    fn dir(&self) -> Result<PathBuf, String> {
        self.dir
            .lock()
            .map_err(|e| format!("lock failed: {e}"))?
            .clone()
            .ok_or_else(|| "app data directory is not set".to_string())
    }

    fn keyring_entry(&self, key: &str) -> Result<Entry, String> {
        Entry::new(KEYRING_SERVICE, key).map_err(|e| format!("keyring entry failed: {e}"))
    }

    fn file_path(&self, key: &str) -> Result<PathBuf, String> {
        // セッションキーをファイル名に安全に変換する（パス区切りを避ける）。
        let safe: String = key
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Ok(self.dir()?.join(format!("novnow-session-{safe}.enc")))
    }

    /// 暗号化ファイル用の鍵を導出する（アプリデータディレクトリ由来の導出値）。
    fn file_key(&self) -> Result<[u8; 32], String> {
        use sha2::{Digest, Sha256};
        let dir = self.dir()?;
        let mut hasher = Sha256::new();
        hasher.update(dir.to_string_lossy().as_bytes());
        hasher.update(FILE_KEY_SALT.as_bytes());
        Ok(hasher.finalize().into())
    }

    /// 暗号化ファイルの読み書き。OS キーチェーンが使えない時のフォールバック。
    fn file_get(&self, key: &str) -> Result<Option<String>, String> {
        let path = self.file_path(key)?;
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            // ファイルが存在しない場合は未登録セッションとして None を返す。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("read failed: {e}")),
        };
        if data.len() < 12 {
            return Err("corrupted encrypted session file".to_string());
        }
        let (nonce_bytes, ct) = data.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&self.file_key()?)
            .map_err(|e| format!("cipher init failed: {e}"))?;
        let nonce = Nonce::from_slice(nonce_bytes);
        let plain = cipher
            .decrypt(nonce, ct)
            .map_err(|e| format!("decrypt failed: {e}"))?;
        String::from_utf8(plain).map(Some).map_err(|e| format!("utf8 failed: {e}"))
    }

    fn file_set(&self, key: &str, value: &str) -> Result<(), String> {
        let path = self.file_path(key)?;
        std::fs::create_dir_all(&self.dir()?)
            .map_err(|e| format!("create dir failed: {e}"))?;
        let cipher = Aes256Gcm::new_from_slice(&self.file_key()?)
            .map_err(|e| format!("cipher init failed: {e}"))?;
        let mut nonce = [0u8; 12];
        getrandom::getrandom(&mut nonce).map_err(|e| format!("nonce failed: {e}"))?;
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce), value.as_bytes())
            .map_err(|e| format!("encrypt failed: {e}"))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        std::fs::write(&path, out).map_err(|e| format!("write failed: {e}"))
    }

    fn file_delete(&self, key: &str) -> Result<(), String> {
        match std::fs::remove_file(self.file_path(key)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("delete failed: {e}")),
        }
    }

    fn get(&self, key: &str) -> Result<Option<String>, String> {
        match self.keyring_entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            // OS キーチェーンが使えない場合は暗号化ファイルへフォールバック。
            Err(_) => self.file_get(key),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        match self.keyring_entry(key)?.set_password(value) {
            Ok(()) => Ok(()),
            // OS キーチェーンが使えない場合は暗号化ファイルへフォールバック。
            Err(_) => self.file_set(key, value),
        }
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        // キーチェーンとフォールバックの両方をクリアする。
        // キーチェーン backend が利用不可でも暗号化ファイル側はクリアする。
        let ke_res = self.keyring_entry(key).and_then(|e| e.delete_credential());
        let file_res = self.file_delete(key);
        if ke_res.is_err() && file_res.is_err() {
            return Err(format!(
                "delete failed: keychain={:?} file={:?}",
                ke_res.err(),
                file_res.err()
            ));
        }
        Ok(())
    }
}

#[tauri::command]
async fn session_get(
    state: tauri::State<'_, SecureStorage>,
    key: String,
) -> Result<Option<String>, String> {
    state.get(&key)
}

#[tauri::command]
async fn session_set(
    state: tauri::State<'_, SecureStorage>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.set(&key, &value)
}

#[tauri::command]
async fn session_delete(
    state: tauri::State<'_, SecureStorage>,
    key: String,
) -> Result<(), String> {
    state.delete(&key)
}

/// カスタムスキーム（`novnow://`）で受信したセットアップ URL を処理する。
///
/// URL にはプロジェクト識別情報のみを含み、認証情報や個人情報は含まない。
/// 作品の取得 → ローカルへの配置 → novnow プロジェクトとしての初期化検証 →
/// エディタ（Web アプリ）の該当ページへ遷移を自動で行う。実体は Web アプリ
/// と共有し、重複実装しない。
#[tauri::command]
fn handle_setup_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let target = format!("/setup/resolve/?u={}", urllib_encode(&url));
    if let Some(window) = app.get_webview_window("main") {
        window
            .navigate(target.parse().map_err(|e| format!("navigate failed: {e}"))?)
            .map_err(|e| format!("navigate failed: {e}"))?;
    }
    Ok(())
}

fn urllib_encode(input: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                let _ = write!(out, "{}", byte as char);
            }
            _ => {
                let _ = write!(out, "%{:02X}", byte);
            }
        }
    }
    out
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // フォールバック（暗号化ファイル）の保存先をアプリのデータディレクトリに設定する。
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("app data dir failed: {e}"))?;
            let state = app.state::<SecureStorage>();
            *state.dir.lock().unwrap() = Some(dir);
            Ok(())
        })
        .manage(SecureStorage::default())
        // カスタムスキーム `novnow://` の受信ハンドラを登録する。
        // Tauri v2 ではハンドラは `http::Response` を返す（`Result` ではない）。
        // 実際の遷移処理は別タスクで非ブロックに行い、応答は空の 200 で即座に返す。
        .register_uri_scheme_protocol("novnow", |ctx, request| {
            let app = ctx.app_handle().clone();
            let url = request.uri().to_string();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = handle_setup_url(app, url) {
                    eprintln!("setup url handling failed: {e}");
                }
            });
            http::Response::builder()
                .status(http::StatusCode::OK)
                .body(Vec::new())
                .expect("failed to build uri scheme response")
        })
        .invoke_handler(tauri::generate_handler![
            session_get,
            session_set,
            session_delete,
            handle_setup_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running novnow-hub")
}
