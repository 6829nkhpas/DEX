// ---------------------------------------------------------------------------
// DexStateStore — deterministic in-memory state store
// ---------------------------------------------------------------------------
//
// Wires pure reducers to a mutable state container. Provides:
//   - dispatch(event) — routes events to the correct reducer
//   - Read-only getters for each domain
//   - State-change listener notification
//   - Bounded dedup guard
// ---------------------------------------------------------------------------

import type { BaseEvent } from "../../../../types/generated-types";
import type {
    StoreState,
    OrderbookState,
    TickerState,
    TradeRecord,
    AccountState,
    SeqMeta,
    StateChangeListener,
    OrderbookSnapshotPayload,
    OrderbookDeltaPayload,
    TickerDeltaPayload,
    TradePayload,
    AccountSnapshotPayload,
    AccountDeltaPayload,
    SnapshotEvent,
    DeltaEvent,
} from "./types";
import {
    isDuplicate,
    recordEvent,
    compareSeq,
    applyOrderbookSnapshot,
    applyOrderbookDelta,
    applyTickerDelta,
    applyTrade,
    applyAccountSnapshot,
    applyAccountDelta,
} from "./reducers";

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/**
 * Deterministic in-memory state store for the DEX frontend.
 *
 * Holds per-symbol orderbooks, tickers, and trades, plus account state.
 * Events are dispatched through pure reducer functions. Duplicates are
 * ignored. Snapshots always reset state (gap-safe).
 */
export class DexStateStore {
    private state: StoreState;
    private seqMeta: Map<string, SeqMeta>; // domain key → dedup metadata
    private listeners: StateChangeListener[] = [];
    private snapshotListeners: ((channel: string, params: Record<string, string>, sinceSeq: number) => void)[] = [];

    // per-stream delta buffer for gaps
    private deltaBuffers: Map<string, BaseEvent<unknown>[]> = new Map();
    private readonly MAX_BUFFER_SIZE = 10_000;

    constructor() {
        this.state = {
            orderbooks: new Map(),
            tickers: new Map(),
            trades: new Map(),
            account: null,
            metrics: {
                events_ignored: 0,
                gaps_detected: 0,
                buffer_size_by_stream: new Map(),
            }
        };
        this.seqMeta = new Map();
    }

    // -----------------------------------------------------------------------
    // Public API — dispatch
    // -----------------------------------------------------------------------

    /**
     * Dispatch an event from the WS client into the state store.
     *
     * Routes by `event.source` (channel) and `event.event_type` (snapshot vs delta).
     * Duplicate events are silently ignored.
     */
    dispatch(event: BaseEvent<unknown>): void {
        const domainKey = this.domainKey(event);
        const meta = this.getSeqMeta(domainKey);

        if (event.event_type === "snapshot") {
            // Apply snapshot atomically, wholesale state replacement
            this.applyEvent(event);

            // Update seenIds and forcefully reset sequence strictly to the snapshot
            const nextMeta = recordEvent(event, meta);
            nextMeta.lastSeq = event.sequence;
            this.seqMeta.set(domainKey, nextMeta);

            // Apply any buffered deltas that now fit in sequence order
            this.flushBuffer(domainKey);
            this.notifyListeners();
            return;
        }

        // Deltas
        if (isDuplicate(event, meta)) {
            this.state.metrics.events_ignored++;
            return;
        }

        const expectedSeq = String(BigInt(meta.lastSeq) + 1n);
        const cmp = compareSeq(event.sequence, expectedSeq);

        if (cmp > 0 && meta.lastSeq !== "0") {
            // > lastSeq + 1 gap detection
            this.state.metrics.gaps_detected++;
            this.bufferDelta(domainKey, event);
            return;
        } else if (cmp > 0 && meta.lastSeq === "0") {
            // Initial snapshot not yet received, buffer it anyway and wait
            this.bufferDelta(domainKey, event);
            return;
        }

        // In-order delta apply
        this.applyEvent(event);
        this.seqMeta.set(domainKey, recordEvent(event, meta));

        // Check if application unlocks further buffered deltas
        this.flushBuffer(domainKey);
        this.notifyListeners();
    }

    private applyEvent(event: BaseEvent<unknown>): void {
        const source = event.source;
        const type = event.event_type;

        switch (source) {
            case "market_data":
                if (type === "snapshot") {
                    this.dispatchOrderbookSnapshot(event as BaseEvent<OrderbookSnapshotPayload>);
                } else if (type === "delta") {
                    this.dispatchMarketDataDelta(event as BaseEvent<TickerDeltaPayload | OrderbookDeltaPayload>);
                }
                break;

            case "trades":
                this.dispatchTrade(event as BaseEvent<TradePayload>);
                break;

            case "account":
                if (type === "snapshot") {
                    this.dispatchAccountSnapshot(event as BaseEvent<AccountSnapshotPayload>);
                } else if (type === "delta") {
                    this.dispatchAccountDelta(event as BaseEvent<AccountDeltaPayload>);
                }
                break;
        }
    }

    private bufferDelta(domainKey: string, event: BaseEvent<unknown>): void {
        let buffer = this.deltaBuffers.get(domainKey);
        if (!buffer) {
            buffer = [];
            this.deltaBuffers.set(domainKey, buffer);
        }

        buffer.push(event);
        this.state.metrics.buffer_size_by_stream.set(domainKey, buffer.length);

        // Cap overflow policy check
        if (buffer.length > this.MAX_BUFFER_SIZE) {
            buffer.length = 0; // Clear it
            this.state.metrics.buffer_size_by_stream.set(domainKey, 0);
            this.triggerSnapshotRequest(event, 0); // Request full snapshot
            return;
        }

        // Otherwise request snapshot since last recorded seq
        const meta = this.getSeqMeta(domainKey);
        this.triggerSnapshotRequest(event, Number(meta.lastSeq));
    }

    private flushBuffer(domainKey: string): void {
        let buffer = this.deltaBuffers.get(domainKey);
        if (!buffer || buffer.length === 0) return;

        // Sort ascending by sequence for in order application
        buffer.sort((a, b) => compareSeq(a.sequence, b.sequence));

        let i = 0;

        while (i < buffer.length) {
            const event = buffer[i];
            const meta = this.getSeqMeta(domainKey);

            if (isDuplicate(event, meta)) {
                this.state.metrics.events_ignored++;
                i++;
                continue;
            }

            const expectedSeq = String(BigInt(meta.lastSeq) + 1n);
            const cmp = compareSeq(event.sequence, expectedSeq);

            if (cmp === 0) {
                // In exact order, apply it
                this.applyEvent(event);
                this.seqMeta.set(domainKey, recordEvent(event, meta));
                i++;
            } else if (cmp > 0) {
                // Another gap remains, wait for snapshot fill
                break;
            }
        }

        // Prune the applied (or discarded) events
        if (i > 0) {
            buffer.splice(0, i);
            this.state.metrics.buffer_size_by_stream.set(domainKey, buffer.length);
        }
    }

    private triggerSnapshotRequest(event: BaseEvent<unknown>, sinceSeq: number): void {
        const payload = event.payload as Record<string, unknown> | null;
        let symbol = "";
        let account_id = "";
        if (payload && typeof payload === "object") {
            if ("symbol" in payload) symbol = String(payload.symbol);
            if ("account_id" in payload) account_id = String(payload.account_id);
        }

        const params: Record<string, string> = {};
        if (symbol) params["symbol"] = symbol;
        if (account_id) params["account_id"] = account_id;

        for (const listener of this.snapshotListeners) {
            listener(event.source, params, sinceSeq);
        }
    }

    // -----------------------------------------------------------------------
    // Public API — read-only getters
    // -----------------------------------------------------------------------

    /** Get the full orderbook for a symbol, or undefined if not yet received. */
    getOrderbook(symbol: string): Readonly<OrderbookState> | undefined {
        return this.state.orderbooks.get(symbol);
    }

    /** Get the ticker for a symbol, or undefined if not yet received. */
    getTicker(symbol: string): Readonly<TickerState> | undefined {
        return this.state.tickers.get(symbol);
    }

    /** Get the trade list for a symbol (most recent last). */
    getTrades(symbol: string): readonly TradeRecord[] {
        return this.state.trades.get(symbol) ?? [];
    }

    /** Get the current account state, or null if not authenticated / no snapshot. */
    getAccount(): Readonly<AccountState> | null {
        return this.state.account;
    }

    /** Get a snapshot of the entire state (read-only). */
    getState(): Readonly<StoreState> {
        return this.state;
    }

    // -----------------------------------------------------------------------
    // Public API — listeners
    // -----------------------------------------------------------------------

    /** Register a callback invoked after every state change. */
    onStateChange(listener: StateChangeListener): () => void {
        this.listeners.push(listener);
        // Return unsubscribe function
        return () => {
            this.listeners = this.listeners.filter((l) => l !== listener);
        };
    }

    /** Register a callback to be invoked when the store needs to request a snapshot (e.g., due to gap). */
    onRequestSnapshot(listener: (channel: string, params: Record<string, string>, sinceSeq: number) => void): () => void {
        this.snapshotListeners.push(listener);
        return () => {
            this.snapshotListeners = this.snapshotListeners.filter((l) => l !== listener);
        };
    }

    // -----------------------------------------------------------------------
    // Internal dispatchers
    // -----------------------------------------------------------------------

    private dispatchOrderbookSnapshot(event: BaseEvent<OrderbookSnapshotPayload>): void {
        const symbol = event.payload.symbol;
        const current = this.state.orderbooks.get(symbol);
        this.state.orderbooks.set(symbol, applyOrderbookSnapshot(current, event));
    }

    private dispatchMarketDataDelta(
        event: BaseEvent<TickerDeltaPayload | OrderbookDeltaPayload>,
    ): void {
        const payload = event.payload;
        const symbol = payload.symbol;

        // If the delta contains bids/asks, it's an orderbook delta
        if ("bids" in payload || "asks" in payload) {
            const current = this.state.orderbooks.get(symbol);
            if (current) {
                this.state.orderbooks.set(
                    symbol,
                    applyOrderbookDelta(current, event as BaseEvent<OrderbookDeltaPayload>),
                );
            }
            // If no current orderbook, ignore delta (need snapshot first)
        }

        // If the delta contains ticker fields, update ticker
        if (
            "last_price" in payload ||
            "volume_24h" in payload ||
            "high_24h" in payload ||
            "low_24h" in payload ||
            "mark_price" in payload
        ) {
            const current = this.state.tickers.get(symbol);
            this.state.tickers.set(
                symbol,
                applyTickerDelta(current, event as BaseEvent<TickerDeltaPayload>),
            );
        }
    }

    private dispatchTrade(event: BaseEvent<TradePayload>): void {
        const symbol = event.payload.symbol;
        const current = this.state.trades.get(symbol) ?? [];
        this.state.trades.set(symbol, applyTrade(current, event));
    }

    private dispatchAccountSnapshot(event: BaseEvent<AccountSnapshotPayload>): void {
        this.state.account = applyAccountSnapshot(this.state.account, event);
    }

    private dispatchAccountDelta(event: BaseEvent<AccountDeltaPayload>): void {
        if (this.state.account) {
            this.state.account = applyAccountDelta(this.state.account, event);
        }
        // If no account snapshot yet, ignore delta (need snapshot first)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /**
     * Build a domain key for sequence/dedup tracking.
     * Groups by source + symbol (for market_data/trades) or source alone (account).
     */
    private domainKey(event: BaseEvent<unknown>): string {
        const payload = event.payload as Record<string, unknown> | null;
        const symbol = payload && typeof payload === "object" && "symbol" in payload
            ? String(payload.symbol)
            : "";
        return symbol ? `${event.source}::${symbol}` : event.source;
    }

    private getSeqMeta(key: string): SeqMeta {
        const existing = this.seqMeta.get(key);
        if (existing) return existing;
        const fresh: SeqMeta = { lastSeq: "0", seenIds: new Set() };
        this.seqMeta.set(key, fresh);
        return fresh;
    }

    private notifyListeners(): void {
        for (const listener of this.listeners) {
            listener(this.state);
        }
    }
}
