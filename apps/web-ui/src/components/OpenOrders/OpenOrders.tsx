// ---------------------------------------------------------------------------
// OpenOrders — live open orders table with per-row cancel button
// ---------------------------------------------------------------------------
//
// Reads account.orders from the store, filters to PENDING / PARTIAL,
// and provides a cancel button that calls DELETE /v1/orders/:id.
//
// IMPORTANT: No optimistic removal — the order stays visible until a WS
// account delta arrives with status CANCELED (or FILLED).
// ---------------------------------------------------------------------------

import React, { useState, useCallback, useRef } from "react";
import { DexApiClient } from "../../api/rest-client";
import { ApiError } from "../../api/types";
import { useDexStore } from "../../state/StoreProvider";
import type { Order } from "../../../../../types/generated-types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface CancelState {
    /** order_id → true while the cancel request is in-flight */
    pending: Record<string, boolean>;
}

export interface OpenOrdersProps {
    /** Optional account ID override. Defaults to "dev-account". */
    accountId?: string;
    /** Optional auth token override */
    token?: string;
}

// ---------------------------------------------------------------------------
// Helpers (exported for tests)
// ---------------------------------------------------------------------------

/** Filter orders to only PENDING and PARTIAL status */
export function filterActiveOrders(orders: Record<string, Order>): Order[] {
    return Object.values(orders).filter(
        (o) => o.status.state === "PENDING" || o.status.state === "PARTIAL",
    );
}

/** Map HTTP error status to user-friendly message */
export function cancelErrorMessage(err: ApiError): string {
    switch (err.status) {
        case 404:
            return "Order not found — it may have already been removed.";
        case 409:
            return "Order already filled or canceled.";
        case 429:
            return "Rate limit exceeded — please try again later.";
        default:
            return err.body?.message ?? err.body?.error ?? `Cancel failed (HTTP ${err.status})`;
    }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const OpenOrders: React.FC<OpenOrdersProps> = ({
    accountId = "dev-account",
    token = "dev-token-123",
}) => {
    const { state } = useDexStore();
    const [cancelState, setCancelState] = useState<CancelState>({ pending: {} });
    const [errorToast, setErrorToast] = useState<string | null>(null);

    // API client singleton
    const clientRef = useRef(new DexApiClient({ baseUrl: "/v1" }));

    // Auto-dismiss error toast after 5s
    const toastTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    const showError = useCallback((msg: string) => {
        setErrorToast(msg);
        if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
        toastTimerRef.current = setTimeout(() => setErrorToast(null), 5000);
    }, []);

    // ---- Cancel handler ----------------------------------------------------

    const handleCancel = useCallback(
        async (orderId: string) => {
            // Guard: already in-flight
            if (cancelState.pending[orderId]) return;

            // Mark pending
            setCancelState((prev) => ({
                pending: { ...prev.pending, [orderId]: true },
            }));

            try {
                await clientRef.current.cancelOrder(
                    orderId,
                    { account_id: accountId },
                    token,
                );
                // Success — do NOT remove from UI; wait for WS event.
            } catch (err: unknown) {
                if (err instanceof ApiError) {
                    showError(cancelErrorMessage(err));
                } else {
                    showError(
                        `Unexpected error: ${err instanceof Error ? err.message : String(err)}`,
                    );
                }
            } finally {
                setCancelState((prev) => {
                    const next = { ...prev.pending };
                    delete next[orderId];
                    return { pending: next };
                });
            }
        },
        [cancelState.pending, accountId, token, showError],
    );

    // ---- Derived data ------------------------------------------------------

    const account = state.account;
    const activeOrders = account ? filterActiveOrders(account.orders) : [];

    // ---- Render -------------------------------------------------------------

    return (
        <div
            id="open-orders"
            style={{
                background: "#111827",
                border: "1px solid #374151",
                borderRadius: 8,
                padding: 16,
                fontFamily: "Inter, system-ui, sans-serif",
                color: "#e5e7eb",
                width: "100%",
            }}
        >
            <h3 style={{ margin: "0 0 12px", fontSize: 16, fontWeight: 600 }}>
                Open Orders
                {activeOrders.length > 0 && (
                    <span
                        style={{
                            marginLeft: 8,
                            fontSize: 12,
                            color: "#9ca3af",
                            fontWeight: 400,
                        }}
                    >
                        ({activeOrders.length})
                    </span>
                )}
            </h3>

            {/* Error toast */}
            {errorToast && (
                <div
                    id="cancel-error"
                    style={{
                        marginBottom: 8,
                        padding: 8,
                        background: "#7f1d1d",
                        borderRadius: 4,
                        fontSize: 13,
                    }}
                >
                    ❌ {errorToast}
                </div>
            )}

            {!account && (
                <div style={{ color: "#6b7280", fontSize: 13 }}>
                    Waiting for account data…
                </div>
            )}

            {account && activeOrders.length === 0 && (
                <div style={{ color: "#6b7280", fontSize: 13 }}>
                    No open orders.
                </div>
            )}

            {activeOrders.length > 0 && (
                <div style={{ overflowX: "auto" }}>
                    <table
                        style={{
                            width: "100%",
                            fontSize: 12,
                            borderCollapse: "collapse",
                            whiteSpace: "nowrap",
                        }}
                    >
                        <thead>
                            <tr style={{ borderBottom: "1px solid #374151" }}>
                                <th style={thStyle}>Order ID</th>
                                <th style={thStyle}>Symbol</th>
                                <th style={thStyle}>Side</th>
                                <th style={thStyle}>Price</th>
                                <th style={thStyle}>Qty</th>
                                <th style={thStyle}>Remaining</th>
                                <th style={thStyle}>Status</th>
                                <th style={thStyle}></th>
                            </tr>
                        </thead>
                        <tbody>
                            {activeOrders.map((order) => {
                                const isPending = cancelState.pending[order.order_id] ?? false;
                                return (
                                    <tr
                                        key={order.order_id}
                                        style={{ borderBottom: "1px solid #1f2937" }}
                                    >
                                        <td style={tdStyle}>
                                            <span title={order.order_id}>
                                                {order.order_id.length > 12
                                                    ? `${order.order_id.slice(0, 8)}…`
                                                    : order.order_id}
                                            </span>
                                        </td>
                                        <td style={tdStyle}>{order.symbol}</td>
                                        <td
                                            style={{
                                                ...tdStyle,
                                                color:
                                                    order.side === "BUY"
                                                        ? "#10b981"
                                                        : "#ef4444",
                                                fontWeight: 600,
                                            }}
                                        >
                                            {order.side}
                                        </td>
                                        <td style={tdStyle}>{order.price}</td>
                                        <td style={tdStyle}>{order.quantity}</td>
                                        <td style={tdStyle}>
                                            {order.remaining_quantity}
                                        </td>
                                        <td style={tdStyle}>
                                            <span
                                                style={{
                                                    color:
                                                        order.status.state === "PARTIAL"
                                                            ? "#fbbf24"
                                                            : "#60a5fa",
                                                }}
                                            >
                                                {order.status.state}
                                            </span>
                                        </td>
                                        <td style={tdStyle}>
                                            <button
                                                id={`cancel-${order.order_id}`}
                                                disabled={isPending}
                                                onClick={() =>
                                                    handleCancel(order.order_id)
                                                }
                                                style={{
                                                    background: isPending
                                                        ? "#374151"
                                                        : "#dc2626",
                                                    color: "#fff",
                                                    border: "none",
                                                    borderRadius: 4,
                                                    padding: "4px 10px",
                                                    fontSize: 11,
                                                    fontWeight: 600,
                                                    cursor: isPending
                                                        ? "not-allowed"
                                                        : "pointer",
                                                    opacity: isPending ? 0.6 : 1,
                                                    minWidth: 60,
                                                }}
                                            >
                                                {isPending ? "⏳…" : "Cancel"}
                                            </button>
                                        </td>
                                    </tr>
                                );
                            })}
                        </tbody>
                    </table>
                </div>
            )}
        </div>
    );
};

// ---------------------------------------------------------------------------
// Inline styles
// ---------------------------------------------------------------------------

const thStyle: React.CSSProperties = {
    textAlign: "left",
    padding: "6px 8px",
    color: "#9ca3af",
    fontWeight: 500,
};

const tdStyle: React.CSSProperties = {
    padding: "6px 8px",
};
