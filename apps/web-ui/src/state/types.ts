// ---------------------------------------------------------------------------
// State types — in-memory state shapes for orderbooks, trades, accounts
// ---------------------------------------------------------------------------

import type {
    Price,
    Quantity,
    Timestamp,
    Side,
    Order,
    BaseEvent,
} from "../../../../types/generated-types";

// ---------------------------------------------------------------------------
// Orderbook
// ---------------------------------------------------------------------------

/** A single price level: [price, quantity]. Quantity "0" means remove. */
export type PriceLevel = [Price, Quantity];

/** Full orderbook state for a single symbol. */
export interface OrderbookState {
    symbol: string;
    bids: PriceLevel[]; // sorted descending by price
    asks: PriceLevel[]; // sorted ascending by price
    lastSeq: string;    // string-encoded integer
}

/** Snapshot payload from WS (event_type "snapshot", source "market_data"). */
export interface OrderbookSnapshotPayload {
    symbol: string;
    bids: PriceLevel[];
    asks: PriceLevel[];
}

/** Delta payload for orderbook level updates. */
export interface OrderbookDeltaPayload {
    symbol: string;
    bids?: PriceLevel[];
    asks?: PriceLevel[];
}

// ---------------------------------------------------------------------------
// Ticker (market data)
// ---------------------------------------------------------------------------

/** Aggregated ticker state for a single symbol. */
export interface TickerState {
    symbol: string;
    last_price: Price;
    volume_24h: Quantity;
    high_24h: Price;
    low_24h: Price;
    mark_price: Price;
    lastSeq: string;
}

/** Delta payload from WS (event_type "delta", source "market_data"). */
export interface TickerDeltaPayload {
    symbol: string;
    last_price?: Price;
    volume_24h?: Quantity;
    high_24h?: Price;
    low_24h?: Price;
    mark_price?: Price;
}

// ---------------------------------------------------------------------------
// Trades
// ---------------------------------------------------------------------------

/** A single recorded trade. */
export interface TradeRecord {
    event_id: string;
    symbol: string;
    price: Price;
    quantity: Quantity;
    side: Side;
    timestamp: Timestamp;
}

/** Trade event payload from WS (source "trades"). */
export interface TradePayload {
    symbol: string;
    price: Price;
    quantity: Quantity;
    side: Side;
}

// ---------------------------------------------------------------------------
// Account
// ---------------------------------------------------------------------------

/** Account state: balances + live order map. */
export interface AccountState {
    account_id: string;
    balances: Record<string, string>; // asset → decimal-as-string
    orders: Record<string, Order>;    // order_id → Order
    lastSeq: string;
}

/** Account snapshot payload. */
export interface AccountSnapshotPayload {
    account_id: string;
    balances: Record<string, string>;
    orders?: Order[];
}

/** Account delta payload (balance or order update). */
export interface AccountDeltaPayload {
    account_id: string;
    balances?: Record<string, string>;
    order?: Order;
}

// ---------------------------------------------------------------------------
// Unified store state
// ---------------------------------------------------------------------------

/** Root state container — all domains. */
export interface StoreState {
    orderbooks: Map<string, OrderbookState>;  // symbol → orderbook
    tickers: Map<string, TickerState>;        // symbol → ticker
    trades: Map<string, TradeRecord[]>;       // symbol → bounded trade list
    account: AccountState | null;
}

// ---------------------------------------------------------------------------
// Sequence / dedup metadata
// ---------------------------------------------------------------------------

/** Per-domain dedup metadata. */
export interface SeqMeta {
    lastSeq: string;        // string-encoded integer — last applied sequence
    seenIds: Set<string>;   // recently seen event_ids for dedup
}

// ---------------------------------------------------------------------------
// Listener
// ---------------------------------------------------------------------------

export type StateChangeListener = (state: Readonly<StoreState>) => void;

// ---------------------------------------------------------------------------
// Event type discriminants used by the store
// ---------------------------------------------------------------------------

export type SnapshotEvent = BaseEvent<OrderbookSnapshotPayload | AccountSnapshotPayload>;
export type DeltaEvent = BaseEvent<
    TickerDeltaPayload | OrderbookDeltaPayload | TradePayload | AccountDeltaPayload
>;
