// ---------------------------------------------------------------------------
// WebSocket message types — derived from ws/ws-protocol.md
// ---------------------------------------------------------------------------

import type { BaseEvent } from "../../../../types/generated-types";

// ---- Channels -------------------------------------------------------------

export type WsChannel = "market_data" | "account" | "trades";

// ---- Client → Server messages ---------------------------------------------

export interface PongMessage {
    type: "pong";
}

export interface SubscribeMessage {
    action: "subscribe";
    channel: WsChannel;
    params: Record<string, string>;
}

export interface UnsubscribeMessage {
    action: "unsubscribe";
    channel: WsChannel;
    params: Record<string, string>;
}

export interface SnapshotSinceMessage {
    action: "snapshot_since";
    channel: WsChannel;
    params: Record<string, string> & { last_seq: number };
}

export type ClientMessage =
    | PongMessage
    | SubscribeMessage
    | UnsubscribeMessage
    | SnapshotSinceMessage;

// ---- Server → Client messages ---------------------------------------------

export interface ConnectedMessage {
    type: "connected";
    session_id: string;
}

export interface PingMessage {
    type: "ping";
}

export interface SubscribedMessage {
    type: "subscribed";
    channel: WsChannel;
    params: Record<string, string>;
    snapshot_seq: number;
}

export interface UnsubscribedMessage {
    type: "unsubscribed";
    channel: WsChannel;
    params: Record<string, string>;
}

export interface SnapshotSinceResponse {
    type: "snapshot_since_response";
    channel: WsChannel;
    from_seq: number;
    to_seq: number;
    events: BaseEvent<unknown>[];
}

export interface WsErrorMessage {
    type: "error";
    code: string;
    message: string;
}

export type ServerMessage =
    | ConnectedMessage
    | PingMessage
    | SubscribedMessage
    | UnsubscribedMessage
    | SnapshotSinceResponse
    | WsErrorMessage
    | BaseEvent<unknown>;

// ---- Subscription tracking ------------------------------------------------

export interface SubscriptionKey {
    channel: WsChannel;
    params: Record<string, string>;
}

export interface SubscriptionState {
    key: SubscriptionKey;
    lastSeq: string; // string-encoded integer
}

// ---- Event handler callback -----------------------------------------------

export type EventHandler<T = unknown> = (event: BaseEvent<T>) => void;

// ---- WS client config -----------------------------------------------------

export interface WsClientConfig {
    /** Full WS URL, e.g. "wss://api.exchange.com/v1/ws" */
    url: string;
    /** Callback to obtain a fresh JWT (called on every connect/reconnect) */
    getToken: () => string | Promise<string>;
}

// ---- Error codes from protocol §7 -----------------------------------------

export type WsErrorCode =
    | "RATE_LIMIT_EXCEEDED"
    | "INVALID_CHANNEL"
    | "AUTH_FAILED"
    | "INVALID_ACTION"
    | "SEQ_TOO_OLD";
