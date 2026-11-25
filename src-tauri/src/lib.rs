//! novnow-hub デスクトップアプリ（Tauri v2）。
//!
//! Web アプリ（Astro + Svelte）と同一のコードベースを WebView で表示し、
//! セッション保管（secure storage）とカスタムスキームでのセットアップ URL
//! 受信を担う。GitHub の概念はアダプター層の背後に隠し、利用者体験には漏らさない。

use std::sync::Mutex;

use tauri::Manager;

/// セッション情報を暗号化保管するバックエンド（stronghold）。
/// OS の Keychain / Credential Manager / Secret Service に暗号化保存し、
/// 平文で露出しない。デスクトップアプリはブラウザ版と異なり、メモリのみに
/// 依存せずアプリ再起動後もセッションを維持する。
#[derive(Default)]
struct SecureStorage(Mutex<stronghold::Stronghold>);

impl SecureStorage {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let guard = self.0.lock().map_err(|e| format!("lock failed: {e}"))?;
        match guard.read_from(SESSION_DB, key) {
            Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
            Err(_) => Ok(None),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let mut guard = self.0.lock().map_err(|e| format!("lock failed: {e}"))?;
        guard
            .write_to(SESSION_DB, key, value.as_bytes())
            .map_err(|e| format!("write failed: {e:?}"))
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        let mut guard = self.0.lock().map_err(|e| format!("lock failed: {e}"))?;
        let _ = guard.delete(SESSION_DB, key);
        Ok(())
    }
}

const SESSION_DB: &str = "novnow-session.sdb";

#[tauri::command]
async fn session_get(
    state: tauri::State<SecureStorage>,
    key: String,
) -> Result<Option<String>, String> {
    state.get(&key)
}

#[tauri::command]
async fn session_set(
    state: tauri::State<SecureStorage>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.set(&key, &value)
}

#[tauri::command]
async fn session_delete(
    state: tauri::State<SecureStorage>,
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
        .manage(SecureStorage::default())
        // カスタムスキーム `novnow://` の受信ハンドラを登録する。
        .register_uri_scheme_protocol("novnow", move |app, request| {
            let url = request.uri().to_string();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = handle_setup_url(app, url) {
                    eprintln!("setup url handling failed: {e}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session_get,
            session_set,
            session_delete,
            handle_setup_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running novnow-hub");
}
