// Generated TypeScript types from centralized_context.json
// Spec version: 1.0.0
// Timestamps: Unix nanoseconds encoded as string to prevent precision loss
// Decimals: All monetary values are decimal-as-string

/** Unix nanoseconds encoded as string (e.g. "1708123456789000000") */
export type Timestamp = string;

/** Decimal-as-string fixed-point price (e.g. "50000.00") */
export type Price = string;

/** Decimal-as-string fixed-point quantity (e.g. "1.0") */
export type Quantity = string;

export enum Side {
    BUY = "BUY",
    SELL = "SELL",
}

export type TimeInForce =
    | { type: "GTC" }
    | { type: "IOC" }
    | { type: "FOK" }
    | { type: "GTD"; value: Timestamp };

export enum CancelReason {
    UserRequested = "USER_REQUESTED",
    SelfTrade = "SELF_TRADE",
    PostOnlyReject = "POST_ONLY_REJECT",
    InsufficientMargin = "INSUFFICIENT_MARGIN",
    RiskLimitBreach = "RISK_LIMIT_BREACH",
    AdminCancel = "ADMIN_CANCEL",
}

export enum RejectReason {
    InvalidSchema = "INVALID_SCHEMA",
    InvalidPrice = "INVALID_PRICE",
    InvalidQuantity = "INVALID_QUANTITY",
    InsufficientBalance = "INSUFFICIENT_BALANCE",
    SymbolNotFound = "SYMBOL_NOT_FOUND",
    AccountSuspended = "ACCOUNT_SUSPENDED",
    RateLimited = "RATE_LIMITED",
}

export type OrderStatus =
    | { state: "PENDING" }
    | { state: "PARTIAL" }
    | { state: "FILLED" }
    | { state: "CANCELED"; reason: CancelReason }
    | { state: "REJECTED"; reason: RejectReason }
    | { state: "EXPIRED" };

/** Complete order structure per spec §1 */
export interface Order {
    order_id: string;
    account_id: string;
    symbol: string;
    side: Side;
    price: Price;
    quantity: Quantity;
    filled_quantity: Quantity;
    remaining_quantity: Quantity;
    status: OrderStatus;
    time_in_force: TimeInForce;
    created_at: Timestamp;
    updated_at: Timestamp;
    version: number;
}

/** Market data snapshot/delta per spec §08 */
export interface MarketData {
    symbol: string;
    last_price: Price;
    volume_24h: Quantity;
    high_24h: Price;
    low_24h: Price;
    mark_price: Price;
}

/** Base event envelope per spec §08 */
export interface BaseEvent<T = unknown> {
    event_id: string;
    event_type: string;
    sequence: string;
    timestamp: Timestamp;
    source: string;
    payload: T;
    metadata: {
        version: string;
        correlation_id: string;
        causation_id: string;
    };
}

/** Account balances */
export interface Account {
    account_id: string;
    balances: Record<string, string>;
}

/** API error response */
export interface ErrorResponse {
    error: string;
    message: string;
}
