/**
 * セッション情報（アクセストークン・更新トークン）の保管抽象化。
 *
 * ブラウザ版とデスクトップ版（Tauri）で挙動を切り分ける：
 * - ブラウザ版: localStorage に保管する（既存の挙動を壊さない）。
 * - デスクトップ版: Tauri secure storage（stronghold、OS の Keychain /
 *   Credential Manager / Secret Service に暗号化保管）に保管し、平文で露出しない。
 *
 * 其余のコードは保管先を知らず、`getAuthHeaders()` のみを呼ぶ。
 */

const ACCESS_TOKEN_KEY = 'novnow.access_token';
const REFRESH_TOKEN_KEY = 'novnow.refresh_token';

/** Tauri 環境かを判定する（window に Tauri の内部オブジェクトが載っているか）。 */
export function isTauri(): boolean {
  return (
    typeof window !== 'undefined' &&
    ('__TAURI_INTERNALS__' in window || '__TAURI__' in window)
  );
}

interface TokenPair {
  accessToken: string;
  refreshToken: string;
}

/** Tauri の invoke を遅延解决し、一度だけ解決して共有する。 */
let tauriInvoke: (<T>(cmd: string, args?: Record<string, unknown>) => Promise<T>) | null = null;
async function getInvoke() {
  if (tauriInvoke) return tauriInvoke;
  const { invoke } = await import('@tauri-apps/api/core');
  tauriInvoke = invoke;
  return invoke;
}

/**
 * Tauri 側のセッション保管コマンドを呼び出す（secure storage 経由）。
 * stronghold で暗号化された値のみを扱い、平文での露出を避ける。
 */
async function tauriGetToken(key: string): Promise<string | null> {
  const invoke = await getInvoke();
  try {
    const value = await invoke<string | null>('session_get', { key });
    return value ?? null;
  } catch {
    // 保管先が未設定・未初期化の場合は空扱い。
    return null;
  }
}

async function tauriSetToken(key: string, value: string): Promise<void> {
  const invoke = await getInvoke();
  await invoke('session_set', { key, value });
}

async function tauriDeleteToken(key: string): Promise<void> {
  const invoke = await getInvoke();
  try {
    await invoke('session_delete', { key });
  } catch {
    // 存在しない項目の削除は無害。
  }
}

function browserGet(key: string): string | null {
  if (typeof localStorage === 'undefined') return null;
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function browserSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage が使えない環境は無害に扱う。
  }
}

function browserDelete(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    // 無害に扱う。
  }
}

/** セッショントークンを保存する。保管先は実行環境に応じて切り分けられる。 */
export async function saveSession(tokens: TokenPair): Promise<void> {
  if (isTauri()) {
    await tauriSetToken(ACCESS_TOKEN_KEY, tokens.accessToken);
    await tauriSetToken(REFRESH_TOKEN_KEY, tokens.refreshToken);
    return;
  }
  browserSet(ACCESS_TOKEN_KEY, tokens.accessToken);
  browserSet(REFRESH_TOKEN_KEY, tokens.refreshToken);
}

/** セッショントークンを取得する。未保管なら undefined。 */
export async function loadSession(): Promise<TokenPair | undefined> {
  if (isTauri()) {
    const [accessToken, refreshToken] = await Promise.all([
      tauriGetToken(ACCESS_TOKEN_KEY),
      tauriGetToken(REFRESH_TOKEN_KEY),
    ]);
    if (accessToken && refreshToken) {
      return { accessToken, refreshToken };
    }
    return undefined;
  }
  const accessToken = browserGet(ACCESS_TOKEN_KEY);
  const refreshToken = browserGet(REFRESH_TOKEN_KEY);
  if (accessToken && refreshToken) {
    return { accessToken, refreshToken };
  }
  return undefined;
}

/** セッショントークンを削除する（ログアウト等）。 */
export async function clearSession(): Promise<void> {
  if (isTauri()) {
    await tauriDeleteToken(ACCESS_TOKEN_KEY);
    await tauriDeleteToken(REFRESH_TOKEN_KEY);
    return;
  }
  browserDelete(ACCESS_TOKEN_KEY);
  browserDelete(REFRESH_TOKEN_KEY);
}

/**
 * 認証ヘッダーを生成する。セッションが無ければ undefined（匿名扱い）。
 * 各ページはこの関数のみを呼び、保管先を知らなくてよい。
 */
export async function getAuthHeaders(): Promise<Record<string, string> | undefined> {
  const session = await loadSession();
  if (!session) return undefined;
  return { Authorization: `Bearer ${session.accessToken}` };
}
