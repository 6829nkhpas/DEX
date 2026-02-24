// ---------------------------------------------------------------------------
// Robust WebSocket client — implements ws/ws-protocol.md in full
// ---------------------------------------------------------------------------

import type { BaseEvent } from "../../../../types/generated-types";
import type {
    WsChannel,
    SubscriptionKey,
    SubscriptionState,
    EventHandler,
    WsClientConfig,
    ServerMessage,
    SnapshotSinceResponse,
} from "./types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a deterministic string key from channel + params for Map lookups. */
function subKey(channel: WsChannel, params: Record<string, string>): string {
    const sorted = Object.keys(params)
        .sort()
        .map((k) => `${k}=${params[k]}`)
        .join("&");
    return `${channel}::${sorted}`;
}

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

/** Compute reconnection delay with exponential backoff + ±20% jitter. */
function backoffDelay(attempt: number): number {
    const base = Math.min(500 * Math.pow(2, attempt), 16_000);
    const jitter = base * 0.2 * (Math.random() * 2 - 1); // ±20%
    return Math.max(0, base + jitter);
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const HEARTBEAT_TIMEOUT_MS = 5_000;

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/**
 * Robust WebSocket client for the DEX exchange.
 *
 * Features implemented per protocol spec:
 *  - JWT auth via query parameter
 *  - Subscribe / unsubscribe with typed channels
 *  - Heartbeat (ping/pong)
 *  - Sequence tracking with **string-encoded integers**
 *  - Gap detection → snapshot-since-seq recovery
 *  - Exponential backoff reconnection (500ms → 16s, ±20% jitter)
 *  - Re-auth + re-subscribe on reconnect
 */
export class DexWebSocketClient {
    // Connection state
    private ws: WebSocket | null = null;
    private sessionId: string | null = null;
    private heartbeatTimer: ReturnType<typeof setTimeout> | null = null;
    private reconnectAttempt = 0;
    private intentionalClose = false;
    private recovering = false;

    // Subscription tracking
    private readonly subscriptions = new Map<string, SubscriptionState>();
    private readonly pendingSubs = new Map<
        string,
        { resolve: () => void; reject: (err: Error) => void }
    >();

    // Event routing
    private readonly handlers = new Map<string, EventHandler[]>();
    private onErrorHandler: ((code: string, msg: string) => void) | null = null;

    constructor(private readonly config: WsClientConfig) { }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /** Open the WebSocket connection. Resolves once the `connected` message is received. */
    async connect(): Promise<void> {
        this.intentionalClose = false;
        return this._connect();
    }

    /** Gracefully close the connection. No reconnect will be attempted. */
    disconnect(): void {
        this.intentionalClose = true;
        this._clearHeartbeat();
        if (this.ws) {
            this.ws.close(1000, "client disconnect");
            this.ws = null;
        }
    }

    /** Subscribe to a channel. Resolves when the server acknowledges. */
    async subscribe(
        channel: WsChannel,
        params: Record<string, string>,
    ): Promise<void> {
        const key = subKey(channel, params);

        // Already subscribed
        if (this.subscriptions.has(key)) return;

        return new Promise<void>((resolve, reject) => {
            this.pendingSubs.set(key, { resolve, reject });
            this._send({
                action: "subscribe" as const,
                channel,
                params,
            });
        });
    }

    /** Unsubscribe from a channel. */
    unsubscribe(channel: WsChannel, params: Record<string, string>): void {
        const key = subKey(channel, params);
        this.subscriptions.delete(key);
        this._send({
            action: "unsubscribe" as const,
            channel,
            params,
        });
    }

    /** Register an event handler for a specific channel. */
    onEvent(channel: WsChannel, handler: EventHandler): void {
        const existing = this.handlers.get(channel) ?? [];
        existing.push(handler);
        this.handlers.set(channel, existing);
    }

    /** Register an error handler for WS protocol errors. */
    onError(handler: (code: string, message: string) => void): void {
        this.onErrorHandler = handler;
    }

    // -----------------------------------------------------------------------
    // Connection lifecycle (private)
    // -----------------------------------------------------------------------

    private async _connect(): Promise<void> {
        const token = await this.config.getToken();
        const url = `${this.config.url}?token=${encodeURIComponent(token)}`;

        return new Promise<void>((resolve, reject) => {
            try {
                this.ws = new WebSocket(url);
            } catch (err) {
                reject(err);
                return;
            }

            this.ws.onopen = () => {
                // Wait for `connected` message — handled in onmessage
            };

            this.ws.onmessage = (ev: MessageEvent) => {
                let msg: ServerMessage;
                try {
                    msg = JSON.parse(String(ev.data)) as ServerMessage;
                } catch {
                    return; // malformed frame — ignore silently
                }
                this._handleMessage(msg, resolve);
            };

            this.ws.onerror = () => {
                // Browser WebSocket fires error then close; we handle in onclose
            };

            this.ws.onclose = () => {
                this._clearHeartbeat();
                if (!this.intentionalClose) {
                    this._scheduleReconnect();
                }
            };
        });
    }

    // -----------------------------------------------------------------------
    // Message dispatcher
    // -----------------------------------------------------------------------

    private _handleMessage(
        msg: ServerMessage,
        connectResolve?: () => void,
    ): void {
        // Narrow by discriminant
        if ("type" in msg) {
            switch (msg.type) {
                case "connected":
                    this.sessionId = msg.session_id;
                    this.reconnectAttempt = 0;
                    this._resetHeartbeat();
                    connectResolve?.();
                    return;

                case "ping":
                    this._resetHeartbeat();
                    this._send({ type: "pong" as const });
                    return;

                case "subscribed": {
                    const key = subKey(msg.channel, msg.params);
                    this.subscriptions.set(key, {
                        key: { channel: msg.channel, params: msg.params },
                        lastSeq: String(msg.snapshot_seq),
                    });
                    const pending = this.pendingSubs.get(key);
                    if (pending) {
                        pending.resolve();
                        this.pendingSubs.delete(key);
                    }
                    return;
                }

                case "unsubscribed": {
                    const key = subKey(msg.channel, msg.params);
                    this.subscriptions.delete(key);
                    return;
                }

                case "snapshot_since_response":
                    this._handleSnapshotSince(msg);
                    return;

                case "error":
                    this.onErrorHandler?.(msg.code, msg.message);
                    return;

                default:
                    break;
            }
        }

        // If it has event_id + sequence, treat as a BaseEvent
        if ("event_id" in msg && "sequence" in msg) {
            this._handleEvent(msg as BaseEvent<unknown>);
        }
    }

    // -----------------------------------------------------------------------
    // Event + sequence handling
    // -----------------------------------------------------------------------

    private _handleEvent(event: BaseEvent<unknown>): void {
        // Determine which subscription this event belongs to
        const channel = event.source as WsChannel;

        // Find matching subscription to track sequence
        for (const [, sub] of this.subscriptions) {
            if (sub.key.channel === channel) {
                const incoming = event.sequence; // string-encoded integer
                const expected = String(BigInt(sub.lastSeq) + 1n);

                if (compareSeq(incoming, expected) > 0 && !this.recovering) {
                    // Gap detected — request recovery
                    this._requestSnapshotSince(sub);
                    return;
                }

                // Update last seen seq (only if >= expected)
                if (compareSeq(incoming, sub.lastSeq) > 0) {
                    sub.lastSeq = incoming;
                }
            }
        }

        // Dispatch to registered handlers
        const handlers = this.handlers.get(channel);
        if (handlers) {
            for (const h of handlers) {
                h(event);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Gap recovery — snapshot-since-seq (§5)
    // -----------------------------------------------------------------------

    private _requestSnapshotSince(sub: SubscriptionState): void {
        this.recovering = true;
        this._send({
            action: "snapshot_since" as const,
            channel: sub.key.channel,
            params: {
                ...sub.key.params,
                last_seq: Number(sub.lastSeq),
            },
        });
    }

    private _handleSnapshotSince(msg: SnapshotSinceResponse): void {
        const channel = msg.channel;

        // Replay each event in order
        for (const event of msg.events) {
            // Update sequence
            for (const [, sub] of this.subscriptions) {
                if (sub.key.channel === channel) {
                    if (compareSeq(event.sequence, sub.lastSeq) > 0) {
                        sub.lastSeq = event.sequence;
                    }
                }
            }

            // Dispatch
            const handlers = this.handlers.get(channel);
            if (handlers) {
                for (const h of handlers) {
                    h(event);
                }
            }
        }

        this.recovering = false;
    }

    // -----------------------------------------------------------------------
    // Heartbeat management
    // -----------------------------------------------------------------------

    private _resetHeartbeat(): void {
        this._clearHeartbeat();
        this.heartbeatTimer = setTimeout(() => {
            // Server didn't ping within expected window — assume stale
            this.ws?.close(4000, "heartbeat timeout");
        }, HEARTBEAT_TIMEOUT_MS + 15_000); // 15s ping interval + 5s tolerance
    }

    private _clearHeartbeat(): void {
        if (this.heartbeatTimer !== null) {
            clearTimeout(this.heartbeatTimer);
            this.heartbeatTimer = null;
        }
    }

    // -----------------------------------------------------------------------
    // Reconnection with exponential backoff (§1.3)
    // -----------------------------------------------------------------------

    private _scheduleReconnect(): void {
        const delay = backoffDelay(this.reconnectAttempt);
        this.reconnectAttempt++;

        setTimeout(async () => {
            try {
                await this._connect();
                // Re-subscribe to all active channels
                await this._resubscribeAll();
            } catch {
                // _connect failed — onclose will fire again → another reconnect
            }
        }, delay);
    }

    private async _resubscribeAll(): Promise<void> {
        // Collect current subs before clearing (we'll get fresh state from server)
        const subs = Array.from(this.subscriptions.values());
        this.subscriptions.clear();

        for (const sub of subs) {
            try {
                await this.subscribe(sub.key.channel, sub.key.params);

                // After resubscribe, request events since last known seq
                const key = subKey(sub.key.channel, sub.key.params);
                const freshSub = this.subscriptions.get(key);
                if (freshSub) {
                    // Check if we missed events between old lastSeq and new snapshot
                    if (compareSeq(sub.lastSeq, "0") > 0) {
                        this._send({
                            action: "snapshot_since" as const,
                            channel: sub.key.channel,
                            params: {
                                ...sub.key.params,
                                last_seq: Number(sub.lastSeq),
                            },
                        });
                        this.recovering = true;
                    }
                }
            } catch {
                // Re-subscribe failed for this channel — will retry on next reconnect
            }
        }
    }

    // -----------------------------------------------------------------------
    // Send helper
    // -----------------------------------------------------------------------

    private _send(msg: Record<string, unknown>): void {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(msg));
        }
    }
}
