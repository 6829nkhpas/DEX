// Generated TypeScript types from centralized JSON context

export type Price = string;
export type Quantity = string;

export enum Side {
    BUY = "BUY",
    SELL = "SELL",
}

export type TimeInForce =
    | { type: "GTC" }
    | { type: "IOC" }
    | { type: "FOK" }
    | { type: "GTD"; value: number };

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

/** Complete order structure */
export interface Order {
    order_id: string; // UUID
    account_id: string; // UUID
    symbol: string; // e.g. "BTC/USDT"
    side: Side;
    price: Price; // Decimal string
    quantity: Quantity; // Decimal string
    filled_quantity: Quantity; // Decimal string
    remaining_quantity: Quantity; // Decimal string
    status: OrderStatus;
    time_in_force: TimeInForce;
    created_at: number; // Unix nanos
    updated_at: number; // Unix nanos
    version: number;
}
