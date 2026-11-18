import type { ReadingDocument } from './api/types';

const cacheName = 'novnow-public-reading-v1';
const cachePath = '/__novnow_cache__/reading';
const memoryVersions = new Map<string, ReadingDocument>();
const memoryLatest = new Map<string, string>();

function memoryVersionKey(projectId: string, etag: string): string {
  return `${projectId}\u0000${etag}`;
}

function browserOrigin(): string {
  if (typeof window !== 'undefined' && window.location.origin !== 'null') {
    return window.location.origin;
  }
  return 'https://novnow-cache.invalid';
}

function versionCacheUrl(projectId: string, etag: string): string {
  return new URL(
    `${cachePath}/${encodeURIComponent(projectId)}/versions/${encodeURIComponent(etag)}`,
    browserOrigin(),
  ).toString();
}

function latestCacheUrl(projectId: string): string {
  return new URL(
    `${cachePath}/${encodeURIComponent(projectId)}/latest`,
    browserOrigin(),
  ).toString();
}

function hasCacheStorage(): boolean {
  return typeof globalThis.caches !== 'undefined';
}

function isReadingDocument(value: unknown, projectId: string, etag: string): value is ReadingDocument {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as Partial<ReadingDocument>;
  return (
    candidate.projectId === projectId &&
    candidate.etag === etag &&
    typeof candidate.title === 'string' &&
    Array.isArray(candidate.blocks)
  );
}

/**
 * Read the latest reader-ready document cached for a public project.
 * The stored representation is versioned by both project ID and the API's ETag.
 */
export async function getCachedReadingDocument(
  projectId: string,
): Promise<ReadingDocument | undefined> {
  if (!projectId) return undefined;

  if (hasCacheStorage()) {
    try {
      const cache = await globalThis.caches.open(cacheName);
      const latest = await cache.match(latestCacheUrl(projectId));
      if (latest) {
        const etag = await latest.text();
        if (etag) {
          const version = await cache.match(versionCacheUrl(projectId, etag));
          if (version) {
            const value: unknown = await version.json();
            if (isReadingDocument(value, projectId, etag)) return value;
          }
        }
      }
    } catch {
      // CacheStorage can be disabled or unavailable; fall through to the memory cache.
    }
  }

  const etag = memoryLatest.get(projectId);
  return etag ? memoryVersions.get(memoryVersionKey(projectId, etag)) : undefined;
}

/** Store a successful public reading response and retire its previous ETag version. */
export async function cacheReadingDocument(document: ReadingDocument): Promise<void> {
  if (!document.projectId || !document.etag) return;

  const previous = await getCachedReadingDocument(document.projectId);
  const oldEtag = previous?.etag;
  const responseBody = JSON.stringify(document);

  if (hasCacheStorage()) {
    try {
      const cache = await globalThis.caches.open(cacheName);
      await cache.put(
        versionCacheUrl(document.projectId, document.etag),
        new Response(responseBody, {
          headers: {
            'Content-Type': 'application/json',
            ETag: document.etag,
          },
        }),
      );
      await cache.put(
        latestCacheUrl(document.projectId),
        new Response(document.etag, { headers: { 'Content-Type': 'text/plain' } }),
      );
      if (oldEtag && oldEtag !== document.etag) {
        await cache.delete(versionCacheUrl(document.projectId, oldEtag));
      }
    } catch {
      // The in-memory cache still provides a best-effort fallback for this app session.
    }
  }

  if (oldEtag && oldEtag !== document.etag) {
    memoryVersions.delete(memoryVersionKey(document.projectId, oldEtag));
  }
  memoryVersions.set(memoryVersionKey(document.projectId, document.etag), document);
  memoryLatest.set(document.projectId, document.etag);
}

/** Remove a project's cached public document, e.g. when the API returns 404. */
export async function clearCachedReadingDocument(projectId: string): Promise<void> {
  if (!projectId) return;

  const current = await getCachedReadingDocument(projectId);
  if (hasCacheStorage()) {
    try {
      const cache = await globalThis.caches.open(cacheName);
      await cache.delete(latestCacheUrl(projectId));
      if (current) await cache.delete(versionCacheUrl(projectId, current.etag));
    } catch {
      // Ignore cache deletion errors; API authorization remains authoritative.
    }
  }

  const etag = memoryLatest.get(projectId);
  if (etag) memoryVersions.delete(memoryVersionKey(projectId, etag));
  memoryLatest.delete(projectId);
}
