// ---------------------------------------------------------------------------
// OpenOrders — live open orders table with per-row cancel button
// ---------------------------------------------------------------------------
//
// Phase 15: improved cancel affordance, consistent empty state, status badges,
//           data-table styling, and better error toast.
// ---------------------------------------------------------------------------

import React, { useState, useCallback, useRef } from "react";
import { DexApiClient } from "../../api/rest-client";
import { ApiError } from "../../api/types";
import { useDexStore } from "../../state/StoreProvider";
import { useAuth } from "../../auth/AuthProvider";
import type { Order } from "../../../../../types/generated-types";
import { EmptyState } from "../ui/EmptyState";
import { LoadingSkeleton } from "../ui/LoadingSkeleton";

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
    accountId: accountIdProp,
    token: tokenProp,
}) => {
    const { authStatus, session } = useAuth();
    const isAuthenticated = authStatus === "authenticated";
    const accountId = session?.accountId ?? accountIdProp ?? "dev-account";
    const token = session?.signature ?? tokenProp ?? "dev-token-123";
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
        <div id="open-orders" className="glass-panel p-6 rounded-2xl w-full flex flex-col gap-4 text-slate-200 shadow-2xl relative overflow-hidden border-t border-indigo-500/20" style={{ gridArea: "orders" }}>
            <div className="flex items-center justify-between">
                <span className="panel-header">
                    Open Orders
                    {activeOrders.length > 0 && (
                        <span className="panel-count">{activeOrders.length}</span>
                    )}
                </span>
            </div>

            {/* Error toast */}
            {errorToast && (
                <div id="cancel-error" className="p-3 bg-rose-500/10 border border-rose-500/30 text-rose-400 rounded-lg text-sm animate-fade-in flex items-center gap-2">
                    <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                    </svg>
                    {errorToast}
                </div>
            )}

            {!account ? (
                <LoadingSkeleton variant="row" count={3} className="rounded-xl border border-indigo-500/10 bg-slate-900/40" />
            ) : activeOrders.length === 0 ? (
                <EmptyState message="No open orders" icon="empty" />
            ) : (
                <div className="overflow-x-auto custom-scrollbar rounded-xl border border-indigo-500/10 bg-slate-900/40">
                    <table className="data-table">
                        <thead>
                            <tr>
                                <th>Order ID</th>
                                <th>Symbol</th>
                                <th>Side</th>
                                <th>Price</th>
                                <th>Qty</th>
                                <th>Remaining</th>
                                <th>Status</th>
                                <th className="text-right">Action</th>
                            </tr>
                        </thead>
                        <tbody>
                            {activeOrders.map((order) => {
                                const isPending = cancelState.pending[order.order_id] ?? false;
                                return (
                                    <tr key={order.order_id} className="group">
                                        <td className="font-mono text-slate-400">
                                            <span title={order.order_id}>
                                                {order.order_id.length > 12
                                                    ? `${order.order_id.slice(0, 8)}…`
                                                    : order.order_id}
                                            </span>
                                        </td>
                                        <td className="font-bold text-white tracking-wide">
                                            {order.symbol}
                                        </td>
                                        <td className={`font-bold ${order.side === "BUY" ? "text-emerald-400" : "text-rose-400"}`}>
                                            {order.side}
                                        </td>
                                        <td className="font-mono text-slate-300 tabular-nums">
                                            {order.price}
                                        </td>
                                        <td className="font-mono text-slate-300 tabular-nums">
                                            {order.quantity}
                                        </td>
                                        <td className="font-mono text-slate-300 tabular-nums">
                                            {order.remaining_quantity}
                                        </td>
                                        <td>
                                            <span className={`status-badge ${order.status.state === "PARTIAL" ? "status-badge-warning" : "status-badge-info"}`}>
                                                {order.status.state}
                                            </span>
                                        </td>
                                        <td className="text-right">
                                            {!isAuthenticated ? (
                                                <span
                                                    className="btn-action btn-action-ghost text-xs opacity-50 cursor-not-allowed py-1 px-2.5"
                                                    title="Sign in to cancel orders"
                                                >
                                                    CANCEL
                                                </span>
                                            ) : (
                                                <button
                                                    id={`cancel-${order.order_id}`}
                                                    disabled={isPending}
                                                    onClick={() => handleCancel(order.order_id)}
                                                    className={`btn-action text-xs py-1 px-2.5 ${isPending ? "btn-action-ghost cursor-not-allowed" : "btn-action-danger"}`}
                                                >
                                                    {isPending ? (
                                                        <span className="flex items-center gap-1">
                                                            <svg className="animate-spin w-3 h-3" fill="none" viewBox="0 0 24 24">
                                                                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                                                                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                                                            </svg>
                                                            Cancelling…
                                                        </span>
                                                    ) : "CANCEL"}
                                                </button>
                                            )}
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
