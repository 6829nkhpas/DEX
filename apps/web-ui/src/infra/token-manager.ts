// ---------------------------------------------------------------------------
// JWT Refresh with mutex — prevents parallel refresh storms
// ---------------------------------------------------------------------------
//
// Implements a thread-safe (single-threaded async-safe) JWT refresh flow.
// When multiple callers need a fresh token simultaneously, only one refresh
// request fires; all others await the same promise.
// ---------------------------------------------------------------------------

export interface TokenPair {
  accessToken: string;
  expiresAt: number; // Unix ms
}

export interface TokenRefreshConfig {
  /** Function that performs the actual token refresh (e.g., POST /auth/refresh). */
  refreshFn: (currentToken: string) => Promise<TokenPair>;
  /** Buffer in ms before expiry to proactively refresh. Default: 30000 (30s) */
  refreshBufferMs: number;
}

const DEFAULT_REFRESH_BUFFER_MS = 30_000;

export class TokenManager {
  private currentToken: string;
  private expiresAt: number;
  private refreshPromise: Promise<TokenPair> | null = null;
  private readonly config: TokenRefreshConfig;

  constructor(
    initialToken: string,
    expiresAt: number,
    config: Pick<TokenRefreshConfig, "refreshFn"> & Partial<TokenRefreshConfig>,
  ) {
    this.currentToken = initialToken;
    this.expiresAt = expiresAt;
    this.config = {
      refreshBufferMs: DEFAULT_REFRESH_BUFFER_MS,
      ...config,
    };
  }

  /**
   * Get a valid token. If the token is expired or about to expire,
   * triggers a refresh (with mutex to avoid parallel refreshes).
   */
  async getToken(): Promise<string> {
    const now = Date.now();
    const needsRefresh = now >= this.expiresAt - this.config.refreshBufferMs;

    if (!needsRefresh) {
      return this.currentToken;
    }

    return this.refreshWithMutex();
  }

  /**
   * Force a token refresh, regardless of expiry.
   */
  async forceRefresh(): Promise<string> {
    return this.refreshWithMutex();
  }

  /**
   * Get the current token without checking expiry (for synchronous access).
   */
  getCurrentToken(): string {
    return this.currentToken;
  }

  /**
   * Check if the token is expired or about to expire.
   */
  isExpired(): boolean {
    return Date.now() >= this.expiresAt - this.config.refreshBufferMs;
  }

  private async refreshWithMutex(): Promise<string> {
    // If a refresh is already in-flight, await it (mutex behavior)
    if (this.refreshPromise) {
      const result = await this.refreshPromise;
      return result.accessToken;
    }

    // Start the refresh
    this.refreshPromise = this.config.refreshFn(this.currentToken);

    try {
      const pair = await this.refreshPromise;
      this.currentToken = pair.accessToken;
      this.expiresAt = pair.expiresAt;
      return pair.accessToken;
    } finally {
      this.refreshPromise = null;
    }
  }
}
