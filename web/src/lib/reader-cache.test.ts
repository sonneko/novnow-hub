import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { clearCachedReadingDocument, cacheReadingDocument, getCachedReadingDocument } from './reader-cache';
import { readingDocumentFixture } from '../test/fixtures';

describe('reader cache', () => {
  const entries = new Map<string, Response>();
  const cache = {
    match: vi.fn(async (request: Request | string) => {
      const key = typeof request === 'string' ? request : request.url;
      return entries.get(key)?.clone();
    }),
    put: vi.fn(async (request: Request | string, response: Response) => {
      const key = typeof request === 'string' ? request : request.url;
      entries.set(key, response.clone());
    }),
    delete: vi.fn(async (request: Request | string) => {
      const key = typeof request === 'string' ? request : request.url;
      return entries.delete(key);
    }),
  };

  beforeEach(async () => {
    entries.clear();
    vi.stubGlobal('caches', { open: vi.fn(async () => cache) });
    await clearCachedReadingDocument(readingDocumentFixture.projectId);
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('keys cached reader data by project ID and ETag, then retires the old version', async () => {
    const firstVersion = { ...readingDocumentFixture, etag: 'W/"reader-v1"' };
    const secondVersion = { ...readingDocumentFixture, etag: 'W/"reader-v2"' };

    await cacheReadingDocument(firstVersion);
    expect(await getCachedReadingDocument(firstVersion.projectId)).toEqual(firstVersion);

    await cacheReadingDocument(secondVersion);
    expect(await getCachedReadingDocument(secondVersion.projectId)).toEqual(secondVersion);
    expect([...entries.keys()].filter((key) => key.includes('/versions/'))).toHaveLength(1);
    expect([...entries.keys()].some((key) => key.includes('reader-v1'))).toBe(false);
    expect([...entries.keys()].some((key) => key.includes('reader-v2'))).toBe(true);
  });
});
