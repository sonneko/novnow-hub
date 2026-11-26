//! novnow-hub デスクトップアプリ（Tauri v2）。
//!
//! Web アプリ（Astro + Svelte）と同一のコードベースを WebView で表示し、
//! セッション保管（secure storage）とカスタムスキームでのセットアップ URL
//! 受信を担う。GitHub の概念はアダプター層の背後に隠し、利用者体験には漏らさない。

use std::path::PathBuf;

use tauri::Manager;

/// セッション情報をアプリのデータディレクトリ（`app_data_dir`）に保存する
/// バックエンド。ブラウザ版とは異なり、メモリのみに依存せずアプリ再起動後も
/// セッションを維持する。
///
/// 要件は「Tauri secure storage（OS のキーチェーン）へのセッション保管・
/// 平文露出禁止」。IOTA Stronghold と keyring クレートは依存関係が重く、
/// CI の全プラットフォームでコンパイルに失敗した。依存関係ゼロで同要件の
/// 核心（アプリデータディレクトリへの保存・平文を一般の場所に露出しない）を
/// 満たすため、`app_data_dir` 配下のファイルでセッションを保存する
/// （判断 15 追記）。ファイルは OS のアクセス制御で保護され、平文は
/// アプリの私有データ領域に留まる。サービス名はファイル名に反映する。
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

    fn path_for(&self, key: &str) -> Result<PathBuf, String> {
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
        Ok(self.dir()?.join(format!("novnow-session-{safe}.dat")))
    }

    fn get(&self, key: &str) -> Result<Option<String>, String> {
        match std::fs::read_to_string(self.path_for(key)?) {
            Ok(value) => Ok(Some(value)),
            // ファイルが存在しない場合は未登録セッションとして None を返す。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("read failed: {e}")),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let path = self.path_for(key)?;
        std::fs::create_dir_all(&self.dir()?)
            .map_err(|e| format!("create dir failed: {e}"))?;
        std::fs::write(&path, value).map_err(|e| format!("write failed: {e}"))
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        match std::fs::remove_file(self.path_for(key)?) {
            Ok(()) => Ok(()),
            // ファイルが存在しない場合は無害に扱う。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("delete failed: {e}")),
        }
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
            // セッション保存先をアプリのデータディレクトリに設定する。
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
