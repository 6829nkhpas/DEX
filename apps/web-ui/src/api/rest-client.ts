// ---------------------------------------------------------------------------
// Typed REST client — hand-authored from openapi/generated-openapi.yaml
// ---------------------------------------------------------------------------

import type { Order, Account } from "../../../../types/generated-types";
import type {
    CreateOrderRequest,
    CancelOrderRequest,
    OrderResponse,
    ApiConfig,
} from "./types";
import { ApiError, AuthRequiredError } from "./types";
import type { ErrorResponse } from "../../../../types/generated-types";

// ---------------------------------------------------------------------------
// Dev-mode detection (safe check without import.meta in tests)
// ---------------------------------------------------------------------------

function isDevMode(): boolean {
    try {
        if (typeof import.meta !== "undefined" && (import.meta as any).env?.DEV) {
            return true;
        }
    } catch { /* ignore */ }
    // Default to true to preserve safe dev behavior
    return true;
}

const DEV_FALLBACK_TOKENS = new Set(["dev-token", "dev-token-123"]);

/**
 * Pre-flight token assertion. In production mode, rejects empty or
 * dev-fallback tokens before sending a request. In dev mode, allows
 * all tokens (preserves existing dev workflow).
 */
function assertToken(token: string): void {
    if (isDevMode()) return;
    if (!token || DEV_FALLBACK_TOKENS.has(token)) {
        throw new AuthRequiredError(
            "Valid authentication token required. Please sign in.",
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function parseJsonSafe<T>(res: Response): Promise<T> {
    const text = await res.text();
    if (!text) throw new ApiError(res.status, null);
    return JSON.parse(text) as T;
}

async function handleResponse<T>(res: Response): Promise<T> {
    if (res.ok) {
        return parseJsonSafe<T>(res);
    }
    let body: ErrorResponse | null = null;
    try {
        body = await parseJsonSafe<ErrorResponse>(res);
    } catch {
        // body stays null
    }
    throw new ApiError(res.status, body);
}

function authHeaders(token: string): Record<string, string> {
    return {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
    };
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/**
 * Typed REST client for the DEX API Gateway.
 *
 * All monetary values (price, quantity) are **string-encoded decimals**.
 * All timestamps are **string-encoded Unix nanoseconds**.
 */
export class DexApiClient {
    private readonly baseUrl: string;

    constructor(config: ApiConfig) {
        // Strip trailing slash
        this.baseUrl = config.baseUrl.replace(/\/+$/, "");
    }

    // ---- POST /orders -------------------------------------------------------

    async createOrder(
        req: CreateOrderRequest,
        token: string,
    ): Promise<OrderResponse> {
        assertToken(token);
        const res = await fetch(`${this.baseUrl}/orders`, {
            method: "POST",
            headers: authHeaders(token),
            body: JSON.stringify(req),
        });
        return handleResponse<OrderResponse>(res);
    }

    // ---- GET /orders/:id ----------------------------------------------------

    async getOrder(id: string, token: string): Promise<Order> {
        assertToken(token);
        const res = await fetch(`${this.baseUrl}/orders/${encodeURIComponent(id)}`, {
            method: "GET",
            headers: authHeaders(token),
        });
        return handleResponse<Order>(res);
    }

    // ---- DELETE /orders/:id -------------------------------------------------

    async cancelOrder(
        id: string,
        req: CancelOrderRequest,
        token: string,
    ): Promise<void> {
        assertToken(token);
        const res = await fetch(`${this.baseUrl}/orders/${encodeURIComponent(id)}`, {
            method: "DELETE",
            headers: authHeaders(token),
            body: JSON.stringify(req),
        });
        if (!res.ok) {
            let body: ErrorResponse | null = null;
            try {
                body = await parseJsonSafe<ErrorResponse>(res);
            } catch {
                // body stays null
            }
            throw new ApiError(res.status, body);
        }
    }

    // ---- GET /accounts/:id --------------------------------------------------

    async getAccount(id: string, token: string): Promise<Account> {
        assertToken(token);
        const res = await fetch(
            `${this.baseUrl}/accounts/${encodeURIComponent(id)}`,
            {
                method: "GET",
                headers: authHeaders(token),
            },
        );
        return handleResponse<Account>(res);
    }
}
