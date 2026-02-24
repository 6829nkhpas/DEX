// ---------------------------------------------------------------------------
// Pure reducers — deterministic state transitions for snapshot & delta events
// ---------------------------------------------------------------------------
//
// Rules:
//   1. applySnapshot() replaces state wholesale — always safe, resets sequence.
//   2. applyDelta() merges into existing state deterministically.
//   3. Duplicate events (same event_id OR sequence ≤ lastSeq) are skipped.
//   4. All numeric values remain as **strings** — no floating-point.
//   5. Functions are pure: they return new objects, never mutate inputs.
// ---------------------------------------------------------------------------

import type { BaseEvent, Order } from "../../../../types/generated-types";
import type {
    PriceLevel,
    OrderbookState,
    OrderbookSnapshotPayload,
    OrderbookDeltaPayload,
    TickerState,
    TickerDeltaPayload,
    TradeRecord,
    TradePayload,
    AccountState,
    AccountSnapshotPayload,
    AccountDeltaPayload,
    SeqMeta,
} from "./types";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Maximum trade records kept per symbol to bound memory. */
const MAX_TRADES_PER_SYMBOL = 500;

/** Maximum seen event IDs in the dedup set before eviction. */
export const MAX_SEEN_IDS = 10_000;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Compare two string-encoded integer sequences.
 * Returns negative if a < b, 0 if equal, positive if a > b.
 */
function compareSeq(a: string, b: string): number {
    const diff = BigInt(a) - BigInt(b);
    if (diff < 0n) return -1;
    if (diff > 0n) return 1;
    return 0;
}

/**
 * Compare two string-encoded decimal prices for sorting.
 * Since prices may have decimals, we compare via parseFloat.
 * This is OK for sort order — all actual values stay as strings.
 */
function comparePriceAsc(a: string, b: string): number {
    return parseFloat(a) - parseFloat(b);
}

/**
 * Merge incoming price levels into existing levels.
 * - If a level has quantity "0", remove it.
 * - Otherwise upsert (replace existing price or insert new).
 * Returns a new sorted array.
 */
function mergeLevels(
    existing: PriceLevel[],
    updates: PriceLevel[],
    sortDescending: boolean,
): PriceLevel[] {
    // Build a map from price → quantity
    const map = new Map<string, string>();
    for (const [price, qty] of existing) {
        map.set(price, qty);
    }
    for (const [price, qty] of updates) {
        if (qty === "0") {
            map.delete(price);
        } else {
            map.set(price, qty);
        }
    }
    // Reconstruct sorted array
    const result: PriceLevel[] = [];
    for (const [price, qty] of map) {
        result.push([price, qty]);
    }
    result.sort((a, b) => {
        const cmp = comparePriceAsc(a[0], b[0]);
        return sortDescending ? -cmp : cmp;
    });
    return result;
}

// ---------------------------------------------------------------------------
// Dedup guard
// ---------------------------------------------------------------------------

/**
 * Check if an event should be skipped (duplicate).
 * Returns `true` if the event is a duplicate and should be ignored.
 */
export function isDuplicate(event: BaseEvent<unknown>, meta: SeqMeta): boolean {
    // Already seen this exact event
    if (meta.seenIds.has(event.event_id)) {
        return true;
    }
    // Sequence already applied (but NOT for snapshots — they always apply)
    if (event.event_type !== "snapshot" && compareSeq(event.sequence, meta.lastSeq) <= 0) {
        return true;
    }
    return false;
}

/**
 * Record that an event was applied. Returns a new SeqMeta.
 * Evicts oldest entries when the set exceeds MAX_SEEN_IDS.
 */
export function recordEvent(event: BaseEvent<unknown>, meta: SeqMeta): SeqMeta {
    const newSeen = new Set(meta.seenIds);
    newSeen.add(event.event_id);

    // Evict oldest if over limit (Set iterates in insertion order)
    if (newSeen.size > MAX_SEEN_IDS) {
        const iter = newSeen.values();
        const toRemove = newSeen.size - MAX_SEEN_IDS;
        for (let i = 0; i < toRemove; i++) {
            const oldest = iter.next().value;
            if (oldest !== undefined) {
                newSeen.delete(oldest);
            }
        }
    }

    const newSeq = compareSeq(event.sequence, meta.lastSeq) > 0
        ? event.sequence
        : meta.lastSeq;

    return { lastSeq: newSeq, seenIds: newSeen };
}

// ---------------------------------------------------------------------------
// Orderbook reducers
// ---------------------------------------------------------------------------

/**
 * Apply a full orderbook snapshot — replaces bids/asks entirely.
 * Always succeeds regardless of current sequence (gap-safe).
 */
export function applyOrderbookSnapshot(
    _current: OrderbookState | undefined,
    event: BaseEvent<OrderbookSnapshotPayload>,
): OrderbookState {
    const { symbol, bids, asks } = event.payload;
    return {
        symbol,
        bids: [...bids].sort((a, b) => comparePriceAsc(b[0], a[0])),  // descending
        asks: [...asks].sort((a, b) => comparePriceAsc(a[0], b[0])),  // ascending
        lastSeq: event.sequence,
    };
}

/**
 * Apply an orderbook delta — merges price levels, removes qty "0" levels.
 */
export function applyOrderbookDelta(
    current: OrderbookState,
    event: BaseEvent<OrderbookDeltaPayload>,
): OrderbookState {
    const { bids: bidUpdates, asks: askUpdates } = event.payload;
    return {
        ...current,
        bids: bidUpdates ? mergeLevels(current.bids, bidUpdates, true) : current.bids,
        asks: askUpdates ? mergeLevels(current.asks, askUpdates, false) : current.asks,
        lastSeq: event.sequence,
    };
}

// ---------------------------------------------------------------------------
// Ticker reducers
// ---------------------------------------------------------------------------

/** Create a fresh ticker from a market data delta (first delta for symbol). */
function freshTicker(event: BaseEvent<TickerDeltaPayload>): TickerState {
    const p = event.payload;
    return {
        symbol: p.symbol,
        last_price: p.last_price ?? "0",
        volume_24h: p.volume_24h ?? "0",
        high_24h: p.high_24h ?? "0",
        low_24h: p.low_24h ?? "0",
        mark_price: p.mark_price ?? "0",
        lastSeq: event.sequence,
    };
}

/**
 * Apply a ticker delta — updates only the fields present in the payload.
 */
export function applyTickerDelta(
    current: TickerState | undefined,
    event: BaseEvent<TickerDeltaPayload>,
): TickerState {
    if (!current) return freshTicker(event);
    const p = event.payload;
    return {
        ...current,
        last_price: p.last_price ?? current.last_price,
        volume_24h: p.volume_24h ?? current.volume_24h,
        high_24h: p.high_24h ?? current.high_24h,
        low_24h: p.low_24h ?? current.low_24h,
        mark_price: p.mark_price ?? current.mark_price,
        lastSeq: event.sequence,
    };
}

// ---------------------------------------------------------------------------
// Trade reducers
// ---------------------------------------------------------------------------

/**
 * Append a trade to the bounded list. Returns a new array.
 */
export function applyTrade(
    current: TradeRecord[],
    event: BaseEvent<TradePayload>,
): TradeRecord[] {
    const record: TradeRecord = {
        event_id: event.event_id,
        symbol: event.payload.symbol,
        price: event.payload.price,
        quantity: event.payload.quantity,
        side: event.payload.side,
        timestamp: event.timestamp,
    };
    const next = [...current, record];
    // Bound the list
    if (next.length > MAX_TRADES_PER_SYMBOL) {
        return next.slice(next.length - MAX_TRADES_PER_SYMBOL);
    }
    return next;
}

// ---------------------------------------------------------------------------
// Account reducers
// ---------------------------------------------------------------------------

/**
 * Apply a full account snapshot — replaces balances and orders.
 */
export function applyAccountSnapshot(
    _current: AccountState | null,
    event: BaseEvent<AccountSnapshotPayload>,
): AccountState {
    const p = event.payload;
    const orders: Record<string, Order> = {};
    if (p.orders) {
        for (const o of p.orders) {
            orders[o.order_id] = o;
        }
    }
    return {
        account_id: p.account_id,
        balances: { ...p.balances },
        orders,
        lastSeq: event.sequence,
    };
}

/**
 * Apply an account delta — merges balance changes and upserts orders.
 */
export function applyAccountDelta(
    current: AccountState,
    event: BaseEvent<AccountDeltaPayload>,
): AccountState {
    const p = event.payload;
    const newBalances = p.balances
        ? { ...current.balances, ...p.balances }
        : current.balances;

    const newOrders = { ...current.orders };
    if (p.order) {
        newOrders[p.order.order_id] = p.order;
    }

    return {
        ...current,
        balances: newBalances,
        orders: newOrders,
        lastSeq: event.sequence,
    };
}
