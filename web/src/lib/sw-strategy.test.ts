import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  CACHE_NAME,
  SHELL_ASSETS,
  cacheFirst,
  isStaticAsset,
  networkFirst,
} from '../../public/sw-strategy.js';

/** jsdom に caches を模倣して注入する。 */
function keyOf(req: Request | string): string {
  return req instanceof Request ? req.url : String(req);
}

function makeFakeCaches(store = new Map<string, Response>()) {
  const cache = {
    match: async (req: Request | string) => store.get(keyOf(req)) ?? null,
    put: async (req: Request | string, res: Response) => {
      store.set(keyOf(req), res);
    },
    delete: async (req: Request | string) => store.delete(keyOf(req)),
  };
  return {
    open: async () => cache,
    match: cache.match,
    delete: cache.delete,
  };
}

describe('sw-strategy', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('exposes a stable cache name and shell assets', () => {
    expect(CACHE_NAME).toBe('novnow-hub-shell-v1/shell');
    expect(SHELL_ASSETS).toContain('/');
    expect(SHELL_ASSETS).toContain('/manifest.json');
  });

  it('classifies static assets', () => {
    expect(isStaticAsset('/_astro/app.js')).toBe(true);
    expect(isStaticAsset('/icons/icon-192.png')).toBe(true);
    expect(isStaticAsset('/styles/global.css')).toBe(true);
    expect(isStaticAsset('/projects/')).toBe(false);
    expect(isStaticAsset('/api/v1/projects')).toBe(false);
  });

  it('returns the cached response when available (cache-first)', async () => {
    const store = new Map<string, Response>([
      ['http://hub.test/index.html', new Response('cached-shell', { status: 200 })],
    ]);
    const fakeCaches = makeFakeCaches(store) as unknown as CacheStorage;
    const fetchImpl = vi.fn().mockRejectedValue(new Error('offline'));
    const request = new Request('http://hub.test/index.html');

    const response = await cacheFirst(request, fetchImpl, fakeCaches);
    expect(response?.status).toBe(200);
    expect(await response?.text()).toBe('cached-shell');
    // キャッシュ命中時はネットワークを呼ばない。
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it('falls back to the cached shell when the network fails and no exact cache exists', async () => {
    // フォールバックは '/index.html'（相対）で参照するため、同一キーで登録する。
    const store = new Map<string, Response>([
      ['/index.html', new Response('fallback-shell', { status: 200 })],
    ]);
    const fakeCaches = makeFakeCaches(store) as unknown as CacheStorage;
    const fetchImpl = vi.fn().mockRejectedValue(new Error('offline'));
    const request = new Request('http://hub.test/projects/');

    const response = await cacheFirst(request, fetchImpl, fakeCaches);
    // フォールバックで '/index.html' のキャッシュが返る。
    expect(response?.status).toBe(200);
    expect(await response?.text()).toBe('fallback-shell');
  });

  it('fetches and caches on cache miss (cache-first)', async () => {
    const store = new Map<string, Response>();
    const fakeCaches = makeFakeCaches(store) as unknown as CacheStorage;
    const fetchImpl = vi.fn().mockResolvedValue(new Response('fresh', { status: 200 }));
    const request = new Request('http://hub.test/styles/global.css');

    const response = await cacheFirst(request, fetchImpl, fakeCaches);
    expect(response?.status).toBe(200);
    expect(await response?.text()).toBe('fresh');
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    // キャッシュに保存される。
    expect(store.has('http://hub.test/styles/global.css')).toBe(true);
  });

  it('prefers network and falls back to cache (network-first)', async () => {
    const store = new Map<string, Response>([
      ['http://hub.test/projects/', new Response('stale', { status: 200 })],
    ]);
    const fakeCaches = makeFakeCaches(store) as unknown as CacheStorage;
    const fetchImpl = vi.fn().mockRejectedValue(new Error('offline'));
    const request = new Request('http://hub.test/projects/');

    const response = await networkFirst(request, fetchImpl, fakeCaches);
    expect(response?.status).toBe(200);
    expect(await response?.text()).toBe('stale');
  });

  it('returns the network response and caches it (network-first)', async () => {
    const store = new Map<string, Response>();
    const fakeCaches = makeFakeCaches(store) as unknown as CacheStorage;
    const fetchImpl = vi.fn().mockResolvedValue(new Response('online', { status: 200 }));
    const request = new Request('http://hub.test/projects/');

    const response = await networkFirst(request, fetchImpl, fakeCaches);
    expect(response?.status).toBe(200);
    expect(await response?.text()).toBe('online');
    expect(store.has('http://hub.test/projects/')).toBe(true);
  });
});
