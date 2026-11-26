/* novnow-hub Service Worker: オフラインシェルを提供する。
 *
 * 戦略:
 * - 静的シェル（HTML・JS・CSS・アイコン）はキャッシュ优先で返し、初回アクセス時に
 *   事前キャッシュし、以降ネットワークが不安定でもシェルを表示できる。
 * - 配信物（公開ページ本文）はネットワーク优先で、失敗時にキャッシュへフォールバック
 *   する。読者ビュー本文の CacheStorage 保存は M3 の reader-cache が担うため、
 *   ここでは重複実装しない（既存の仕組みを壊さない）。
 * - API リクエスト（/api/v1）はネットワークに委ね、キャッシュしない。
 */
import { CACHE_NAME, SHELL_ASSETS, isStaticAsset, cacheFirst, networkFirst } from './sw-strategy.js';

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE_NAME)
      .then((cache) => cache.addAll(SHELL_ASSETS))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key))),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  // API リクエストはキャッシュせずネットワークに委ねる。
  if (url.pathname.startsWith('/api/')) return;
  // ドキュメントナビゲーション（HTML）と静的アセットはキャッシュ优先。
  if (request.mode === 'navigate' || isStaticAsset(url.pathname)) {
    event.respondWith(cacheFirst(request));
    return;
  }
  // その他の GET はネットワーク优先・キャッシュフォールバック。
  event.respondWith(networkFirst(request));
});

export { CACHE_NAME, SHELL_ASSETS, isStaticAsset, cacheFirst, networkFirst };
