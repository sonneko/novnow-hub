//! novnow-hub デスクトップアプリ（Tauri v2）。
//!
//! Web アプリ（Astro + Svelte）と同一のコードベースを WebView で表示し、
//! セッション保管（secure storage）とカスタムスキームでのセットアップ URL
//! 受信を担う。GitHub の概念はアダプター層の背後に隠し、利用者体験には漏らさない。

use std::sync::Mutex;

use tauri::Manager;

/// セッション情報を暗号化保管するバックエンド（IOTA Stronghold）。
/// OS の Keychain / Credential Manager / Secret Service に暗号化保存し、
/// 平文で露出しない。デスクトップアプリはブラウザ版と異なり、メモリのみに
/// 依存せずアプリ再起動後もセッションを維持する。
///
/// `iota_stronghold::Stronghold` は内部で `Store` を保持し、`store()` 経由で
/// キーバイトの読み書きを行う（同期 API）。
#[derive(Default)]
struct SecureStorage(Mutex<iota_stronghold::Stronghold>);

impl SecureStorage {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let guard = self.0.lock().map_err(|e| format!("lock failed: {e}"))?;
        match guard.store().get(key.as_bytes()) {
            Ok(Some(bytes)) => Ok(String::from_utf8(bytes).ok()),
            Ok(None) => Ok(None),
            Err(e) => Err(format!("read failed: {e}")),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let guard = self.0.lock().map_err(|e| format!("lock failed: {e}"))?;
        guard
            .store()
            .insert(key.as_bytes().to_vec(), value.as_bytes().to_vec(), None)
            .map_err(|e| format!("write failed: {e}"))?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        let guard = self.0.lock().map_err(|e| format!("lock failed: {e}"))?;
        // 存在しない項目の削除は無害に扱う。
        let _ = guard.store().delete(key.as_bytes());
        Ok(())
    }
}

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
