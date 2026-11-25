/**
 * novnow-hub Service Worker のキャッシュ戦略（テスト可能な純粋関数群）。
 *
 * service worker 本体（sw.js）とテストから共有する。
 */

export const CACHE_NAME = 'novnow-hub-shell-v1/shell';

export const SHELL_ASSETS = [
  '/',
  '/index.html',
  '/manifest.json',
  '/icons/icon-192.png',
  '/icons/icon-512.png',
];

export function isStaticAsset(pathname) {
  return (
    /\.(?:js|css|png|jpe?g|svg|webp|ico|woff2?)$/i.test(pathname) ||
    pathname.startsWith('/icons/')
  );
}

/** 静的シェルはキャッシュ优先。ネットワーク失敗時はキャッシュ済みシェルへフォールバック。 */
export async function cacheFirst(request, fetchImpl = fetch, cachesImpl = caches) {
  const cached = await cachesImpl.match(request);
  if (cached) return cached;
  try {
    const response = await fetchImpl(request);
    if (response && response.ok) {
      const cache = await cachesImpl.open(CACHE_NAME);
      cache.put(request, response.clone());
    }
    return response;
  } catch {
    const fallback = await cachesImpl.match('/index.html');
    if (fallback) return fallback;
    throw new Error('offline-shell-unavailable');
  }
}

/** その他の GET はネットワーク优先・キャッシュフォールバック。 */
export async function networkFirst(request, fetchImpl = fetch, cachesImpl = caches) {
  try {
    const response = await fetchImpl(request);
    if (response && response.ok) {
      const cache = await cachesImpl.open(CACHE_NAME);
      cache.put(request, response.clone());
    }
    return response;
  } catch {
    const cached = await cachesImpl.match(request);
    if (cached) return cached;
    throw new Error('offline-unavailable');
  }
}
