export type ApiFailure =
  | 'auth-required'
  | 'not-found'
  | 'conflict'
  | 'rate-limited'
  | 'upstream-unavailable'
  | 'verification-failed'
  | 'unknown';

function problemCode(error: unknown): string | undefined {
  if (typeof error !== 'object' || error === null || !('code' in error)) {
    return undefined;
  }

  const code = error.code;
  return typeof code === 'string' ? code : undefined;
}

/** Convert RFC 9457 problem codes/statuses to the UI's API-independent states. */
export function mapApiFailure(error: unknown, status: number): ApiFailure {
  switch (problemCode(error)) {
    case 'authentication_required':
    case 'session_expired':
    case 'refresh_token_reused':
      return 'auth-required';
    case 'not_found':
    case 'job_not_found':
      return 'not-found';
    case 'conflict':
    case 'idempotency_mismatch':
      return 'conflict';
    case 'rate_limited':
      return 'rate-limited';
    case 'upstream_unavailable':
      return 'upstream-unavailable';
    case 'verification_failed':
      return 'verification-failed';
    default:
      if (status === 401) return 'auth-required';
      if (status === 404) return 'not-found';
      if (status === 409) return 'conflict';
      if (status === 429) return 'rate-limited';
      if (status >= 500) return 'upstream-unavailable';
      return 'unknown';
  }
}
