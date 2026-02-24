// ---------------------------------------------------------------------------
// API types — re-exports shared types + adds request/response DTOs from OpenAPI
// ---------------------------------------------------------------------------

export type {
    Timestamp,
    Price,
    Quantity,
    Side,
    TimeInForce,
    OrderStatus,
    Order,
    MarketData,
    Account,
    ErrorResponse,
    BaseEvent,
} from "../../../../types/generated-types";

import type { Side, TimeInForce, Price, Quantity, ErrorResponse } from "../../../../types/generated-types";

// ---- Request DTOs (from OpenAPI components/schemas) -----------------------

/** POST /orders */
export interface CreateOrderRequest {
    account_id: string;
    symbol: string;
    side: Side;
    price: Price;
    quantity: Quantity;
    time_in_force: TimeInForce;
}

/** DELETE /orders/:id */
export interface CancelOrderRequest {
    account_id: string;
}

// ---- Response DTOs --------------------------------------------------------

/** POST /orders → 200 */
export interface OrderResponse {
    order_id: string;
    status: string;
}

// ---- Client configuration -------------------------------------------------

export interface ApiConfig {
    /** Base URL, e.g. "https://api.exchange.com/v1" */
    baseUrl: string;
}

// ---- Error wrapper --------------------------------------------------------

export class ApiError extends Error {
    constructor(
        public readonly status: number,
        public readonly body: ErrorResponse | null,
    ) {
        super(body?.message ?? `HTTP ${status}`);
        this.name = "ApiError";
    }
}
