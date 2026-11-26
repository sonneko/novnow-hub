import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// @tauri-apps/api/core の invoke を、テスト内で操作可能なモックに差し替える。
const tauriStore = new Map<string, string>();
const invokeMock = vi.fn(async (cmd: string, args?: { key?: string; value?: string }) => {
  if (cmd === 'session_get') return tauriStore.get(args?.key ?? '') ?? null;
  if (cmd === 'session_set') {
    tauriStore.set(args!.key!, args!.value!);
    return undefined;
  }
  if (cmd === 'session_delete') {
    tauriStore.delete(args?.key ?? '');
    return undefined;
  }
  throw new Error(`unknown command ${cmd}`);
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

import {
  clearSession,
  getAuthHeaders,
  isTauri,
  loadSession,
  saveSession,
} from './session-storage';

describe('session-storage', () => {
  beforeEach(() => {
    localStorage.clear();
    tauriStore.clear();
    invokeMock.mockClear();
  });

  afterEach(() => {
    localStorage.clear();
    // `in` 演算子は値に関わらず存在を検出するため、プロパティを完全に除去する。
    const w = window as unknown as Record<string, unknown>;
    try {
      delete w['__TAURI_INTERNALS__'];
    } catch {
      Object.defineProperty(window, '__TAURI_INTERNALS__', {
        value: undefined,
        configurable: true,
        writable: true,
      });
    }
  });

  it('reports false for isTauri in a browser environment', () => {
    expect(isTauri()).toBe(false);
  });

  it('stores and loads a session via localStorage in the browser', async () => {
    await saveSession({ accessToken: 'at-1', refreshToken: 'rt-1' });
    const session = await loadSession();
    expect(session).toEqual({ accessToken: 'at-1', refreshToken: 'rt-1' });
    // ブラウザ環境では Tauri の invoke は呼ばれない。
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('returns undefined when no session is stored', async () => {
    expect(await loadSession()).toBeUndefined();
  });

  it('produces a Bearer auth header from the stored session', async () => {
    await saveSession({ accessToken: 'at-2', refreshToken: 'rt-2' });
    const headers = await getAuthHeaders();
    expect(headers).toEqual({ Authorization: 'Bearer at-2' });
  });

  it('returns no header when there is no session', async () => {
    expect(await getAuthHeaders()).toBeUndefined();
  });

  it('clears the session on clearSession', async () => {
    await saveSession({ accessToken: 'at-3', refreshToken: 'rt-3' });
    await clearSession();
    expect(await loadSession()).toBeUndefined();
    expect(await getAuthHeaders()).toBeUndefined();
  });

  it('delegates storage to Tauri secure storage when running under Tauri', async () => {
    // Tauri 環境をシミュレートする。
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      value: {},
      configurable: true,
    });
    expect(isTauri()).toBe(true);

    await saveSession({ accessToken: 'tauri-at', refreshToken: 'tauri-rt' });
    // localStorage に平文で落ちていないこと。
    expect(localStorage.getItem('novnow.access_token')).toBeNull();
    expect(tauriStore.get('novnow.access_token')).toBe('tauri-at');
    expect(tauriStore.get('novnow.refresh_token')).toBe('tauri-rt');

    const session = await loadSession();
    expect(session).toEqual({ accessToken: 'tauri-at', refreshToken: 'tauri-rt' });

    const headers = await getAuthHeaders();
    expect(headers).toEqual({ Authorization: 'Bearer tauri-at' });

    await clearSession();
    expect(await loadSession()).toBeUndefined();
    expect(tauriStore.has('novnow.access_token')).toBe(false);
  });
});
