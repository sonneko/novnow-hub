import createClient from 'openapi-fetch';
import type { paths } from './generated/schema';

const defaultBaseUrl = import.meta.env.PUBLIC_API_BASE_URL || '/api/v1';

/** OpenAPI-generated paths provide the only endpoint/type surface for requests. */
export function createApiClient(baseUrl: string = defaultBaseUrl) {
  return createClient<paths>({
    baseUrl,
    // Resolve fetch at request time so browser service workers and test interceptors apply.
    fetch: (request: Request) => globalThis.fetch(request),
  });
}

export const apiClient = createApiClient();
