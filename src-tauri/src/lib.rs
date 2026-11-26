//! novnow-hub デスクトップアプリ（Tauri v2）。
//!
//! Web アプリ（Astro + Svelte）と同一のコードベースを WebView で表示し、
//! セッション保管（secure storage）とカスタムスキームでのセットアップ URL
//! 受信を担う。GitHub の概念はアダプター層の背後に隠し、利用者体験には漏らさない。

use std::path::PathBuf;
use std::sync::LazyLock;

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
/// 記録に残し、**暗号化ファイル**（AES-256-GCM）へフォールバックする。
/// **平文は残さない**。
///
/// 暗号化ファイルの鍵は、**マシン固有のエントロピー（machine-id 等）とアプリ
/// データディレクトリのパスと定数ソルトを合成して導出**する。パス単体からは
/// 鍵が導出できないようにするため、マシン固有値を KDF に混ぜ込む（判断 18）。
const KEYRING_SERVICE: &str = "novnow-hub";
/// 暗号化ファイルの鍵導出に用いるソルト（定数）。KDF 変更時に更新する。
const FILE_KEY_SALT: &str = "novnow-hub:session-key:v2";

/// マシン固有のエントロピー（machine-id 等）。鍵導出に混ぜ込み、パス単体からの
/// 鍵導出を防止する。プロセス内で 1 回だけ計算してキャッシュする。
///
/// 水源はプラットフォーム別に順次試行し、最終手段でホスト名を用いる。
/// macOS は `ioreg`、Windows は `reg` でマシン識別子を取得する（コンパイル時
/// は各プラットフォームのブロックのみが収められるため、全プラットフォームで
/// コンパイルできる）。
static MACHINE_ENTROPY: LazyLock<String> = LazyLock::new(machine_entropy);

/// マシン固有のエントロピー文字列を取得する。
fn machine_entropy() -> String {
    #[cfg(target_os = "linux")]
    {
        for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(s) = std::fs::read_to_string(path) {
                let s = s.trim().to_string();
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(uuid) = parse_ioplatform_uuid(&text) {
                return uuid;
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(out) = std::process::Command::new("reg")
            .args([
                "query",
                r"HKLM\SOFTWARE\Microsoft\Cryptography",
                "/v",
                "MachineGuid",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(guid) = parse_machine_guid(&text) {
                return guid;
            }
        }
    }
    // 最終手段: ホスト名。マシン特定性は弱いが、パス単体よりは鍵の導出を困難にする。
    std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "novnow-hub-fallback".to_string())
}

/// `ioreg` 出力から `IOPlatformUUID` の値を取り出す。
#[cfg(target_os = "macos")]
fn parse_ioplatform_uuid(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(idx) = line.find("IOPlatformUUID") {
            let rest = &line[idx + "IOPlatformUUID".len()..];
            if let Some(start) = rest.find('"') {
                let after = &rest[start + 1..];
                if let Some(end) = after.find('"') {
                    let value = after[..end].to_string();
                    if !value.is_empty() {
                        return Some(value);
                    }
                }
            }
        }
    }
    None
}

/// `reg query` 出力から `MachineGuid` の値を取り出す。
#[cfg(target_os = "windows")]
fn parse_machine_guid(text: &str) -> Option<String> {
    for line in text.lines() {
        if line.to_ascii_uppercase().contains("MACHINEGUID") {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            // 形式: `MachineGuid    REG_SZ    {GUID}` → 値は 3 番目のトークン
            if tokens.len() >= 3 {
                let value = tokens[2].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// 暗号化鍵を導出する純粋関数（テスト容易のために分離）。
/// `dir` とマシン固有エントロピーを合成して SHA-256 を適用する。
fn derive_key(dir: &std::path::Path, machine: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(dir.to_string_lossy().as_bytes());
    hasher.update(machine.as_bytes());
    hasher.update(FILE_KEY_SALT.as_bytes());
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

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

    /// 暗号化ファイル用の鍵を導出する。
    /// マシン固有エントロピー＋アプリデータディレクトリ＋定数ソルトを合成し、
    /// パス単体からの鍵導出を防止する（判断 18）。
    fn file_key(&self) -> Result<[u8; 32], String> {
        Ok(derive_key(&self.dir()?, &MACHINE_ENTROPY))
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
        let ke_res = self
            .keyring_entry(key)
            .and_then(|e| e.delete_credential().map_err(|err| format!("keychain delete failed: {err}")));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 鍵導出が決定的であること（同一入力→同一鍵）。
    #[test]
    fn key_derivation_is_deterministic() {
        let dir = Path::new("/tmp/novnow-test-dir");
        let k1 = derive_key(dir, "machine-a");
        let k2 = derive_key(dir, "machine-a");
        assert_eq!(k1, k2, "同一入力で鍵が一致しない");
    }

    /// 鍵がパス単体から導出できないこと（マシン固有値が反映されている）。
    #[test]
    fn key_not_derivable_from_path_only() {
        let dir = Path::new("/tmp/novnow-test-dir");
        let k_a = derive_key(dir, "machine-a");
        let k_b = derive_key(dir, "machine-b");
        assert_ne!(
            k_a, k_b,
            "マシン固有値が反映されておらず、パス単体から鍵が導出できている"
        );
    }

    /// パスが異なれば鍵も異なること。
    #[test]
    fn key_differs_by_dir() {
        let k1 = derive_key(Path::new("/tmp/novnow-a"), "machine-a");
        let k2 = derive_key(Path::new("/tmp/novnow-b"), "machine-a");
        assert_ne!(k1, k2);
    }

    /// 暗号化・復号の往復と、平文が残らないこと。
    #[test]
    fn encrypt_decrypt_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("novnow-roundtrip-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("create temp dir");
        let mut storage = SecureStorage::default();
        *storage.dir.lock().unwrap() = Some(tmp.clone());

        let key = "access_token";
        let value = "secret-token-value-123";

        storage.file_set(key, value).expect("file_set");
        let got = storage.file_get(key).expect("file_get");
        assert_eq!(got.as_deref(), Some(value), "復号結果が一致しない");

        // 平文がファイルに残っていないこと。
        let raw = std::fs::read(storage.file_path(key).unwrap()).expect("read enc file");
        assert!(
            !raw.windows(value.len()).any(|w| w == value.as_bytes()),
            "暗号化ファイルに平文が残っている"
        );

        // 削除が動作すること。
        storage.file_delete(key).expect("file_delete");
        assert_eq!(storage.file_get(key).expect("file_get after delete"), None);

        std::fs::remove_dir_all(&tmp).ok();
    }
}
