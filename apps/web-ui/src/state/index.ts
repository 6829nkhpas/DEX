// ---------------------------------------------------------------------------
// Barrel export — public API for the state layer
// ---------------------------------------------------------------------------

// Types
export type {
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
    StoreState,
    SeqMeta,
    StateChangeListener,
} from "./types";

// Pure reducers
export {
    isDuplicate,
    recordEvent,
    applyOrderbookSnapshot,
    applyOrderbookDelta,
    applyTickerDelta,
    applyTrade,
    applyAccountSnapshot,
    applyAccountDelta,
} from "./reducers";

// Store class
export { DexStateStore } from "./store";
