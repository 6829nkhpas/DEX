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
import { useAuth } from "../../auth/AuthProvider";
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
        <div id="open-orders" className="glass-panel p-6 rounded-2xl w-full flex flex-col gap-4 text-slate-200 shadow-2xl relative overflow-hidden border-t border-indigo-500/20">
            <h3 className="text-xl font-display font-bold tracking-tight text-white m-0 flex items-center gap-2">
                Open Orders
                {activeOrders.length > 0 && (
                    <span className="px-2 py-0.5 rounded-full bg-indigo-500/20 text-indigo-400 text-xs font-semibold">
                        {activeOrders.length}
                    </span>
                )}
            </h3>

            {/* Error toast */}
            {errorToast && (
                <div id="cancel-error" className="p-3 bg-rose-500/10 border border-rose-500/30 text-rose-400 rounded-lg text-sm animate-fade-in flex items-center gap-2 mb-2">
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" /></svg>
                    {errorToast}
                </div>
            )}

            {!account ? (
                <div className="text-slate-500 py-8 text-center font-medium bg-slate-900/30 rounded-xl border border-indigo-500/10 flex items-center justify-center gap-2">
                    <div className="w-4 h-4 rounded-full border-2 border-slate-500 border-t-transparent animate-spin" />
                    Waiting for account data…
                </div>
            ) : activeOrders.length === 0 ? (
                <div className="text-slate-500 py-8 text-center font-medium bg-slate-900/30 rounded-xl border border-indigo-500/10">
                    No open orders.
                </div>
            ) : (
                <div className="overflow-x-auto custom-scrollbar rounded-xl border border-indigo-500/10 bg-slate-900/40">
                    <table className="w-full text-left text-sm whitespace-nowrap">
                        <thead className="bg-slate-800/50 text-xs text-slate-500 uppercase tracking-wider">
                            <tr>
                                <th className="px-5 py-3 font-semibold">Order ID</th>
                                <th className="px-5 py-3 font-semibold">Symbol</th>
                                <th className="px-5 py-3 font-semibold">Side</th>
                                <th className="px-5 py-3 font-semibold">Price</th>
                                <th className="px-5 py-3 font-semibold">Qty</th>
                                <th className="px-5 py-3 font-semibold">Remaining</th>
                                <th className="px-5 py-3 font-semibold">Status</th>
                                <th className="px-5 py-3 font-semibold text-right">Action</th>
                            </tr>
                        </thead>
                        <tbody className="divide-y divide-indigo-500/10">
                            {activeOrders.map((order) => {
                                const isPending = cancelState.pending[order.order_id] ?? false;
                                return (
                                    <tr
                                        key={order.order_id}
                                        className="hover:bg-indigo-500/5 transition-colors group"
                                    >
                                        <td className="px-5 py-3 font-mono text-slate-400">
                                            <span title={order.order_id}>
                                                {order.order_id.length > 12
                                                    ? `${order.order_id.slice(0, 8)}…`
                                                    : order.order_id}
                                            </span>
                                        </td>
                                        <td className="px-5 py-3 font-bold text-white tracking-wide">
                                            {order.symbol}
                                        </td>
                                        <td className={`px-5 py-3 font-bold ${order.side === "BUY" ? "text-emerald-400" : "text-rose-400"}`}>
                                            {order.side}
                                        </td>
                                        <td className="px-5 py-3 font-mono text-slate-300">
                                            {order.price}
                                        </td>
                                        <td className="px-5 py-3 font-mono text-slate-300">
                                            {order.quantity}
                                        </td>
                                        <td className="px-5 py-3 font-mono text-slate-300">
                                            {order.remaining_quantity}
                                        </td>
                                        <td className="px-5 py-3">
                                            <span className={`px-2 py-1 rounded border text-xs font-semibold ${order.status.state === "PARTIAL" ? "bg-amber-500/10 border-amber-500/20 text-amber-400" : "bg-blue-500/10 border-blue-500/20 text-blue-400"}`}>
                                                {order.status.state}
                                            </span>
                                        </td>
                                        <td className="px-5 py-3 text-right">
                                            {!isAuthenticated ? (
                                                <span
                                                    className="px-3 py-1.5 rounded-lg text-xs font-bold border bg-slate-800 border-slate-700 text-slate-500 cursor-not-allowed"
                                                    title="Sign in to cancel orders"
                                                >
                                                    CANCEL
                                                </span>
                                            ) : (
                                                <button
                                                    id={`cancel-${order.order_id}`}
                                                    disabled={isPending}
                                                    onClick={() => handleCancel(order.order_id)}
                                                    className={`px-3 py-1.5 rounded-lg text-xs font-bold transition-all border ${isPending ? "bg-slate-800 border-slate-700 text-slate-400 cursor-not-allowed" : "bg-rose-500/20 border-rose-500/30 text-rose-400 hover:bg-rose-500 hover:text-white hover:border-rose-500 shadow-sm"}`}
                                                >
                                                    {isPending ? "CANCELLING…" : "CANCEL"}
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
