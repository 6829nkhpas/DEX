// ---------------------------------------------------------------------------
// OrderEntry — order submission form with validation, REST integration,
//              local submitted-orders tracking, and rate-limit handling
// ---------------------------------------------------------------------------

import React, { useState, useCallback, useRef, useEffect } from "react";
import Decimal from "decimal.js";
import { DexApiClient } from "../../api/rest-client";
import { ApiError } from "../../api/types";
import type { CreateOrderRequest, OrderResponse } from "../../api/types";
import type { Side, TimeInForce } from "../../../../../types/generated-types";
import { useDexStore } from "../../state/StoreProvider";

// ---------------------------------------------------------------------------
// Validation helpers  (exported for unit tests)
// ---------------------------------------------------------------------------

export interface ValidationErrors {
    price?: string;
    quantity?: string;
    side?: string;
    order_type?: string;
    time_in_force?: string;
    gtd_date?: string;
}

/** Returns true if `v` is a valid decimal > 0 */
export function isPositiveDecimal(v: string): boolean {
    if (!v || v.trim() === "") return false;
    try {
        const d = new Decimal(v);
        return d.isFinite() && d.gt(0);
    } catch {
        return false;
    }
}

/** Returns true if `v` is a valid decimal string (including zero). */
export function isValidDecimal(v: string): boolean {
    if (!v || v.trim() === "") return false;
    try {
        const d = new Decimal(v);
        return d.isFinite();
    } catch {
        return false;
    }
}

export function validateOrder(fields: {
    side: string;
    order_type: string;
    price: string;
    quantity: string;
    tif: string;
    gtdDate: string;
}): ValidationErrors {
    const errors: ValidationErrors = {};

    if (!fields.side) errors.side = "Side is required";
    if (!fields.order_type) errors.order_type = "Order type is required";

    if (!fields.quantity) {
        errors.quantity = "Quantity is required";
    } else if (!isPositiveDecimal(fields.quantity)) {
        errors.quantity = "Quantity must be a positive decimal number";
    }

    // Price required for LIMIT orders
    if (fields.order_type === "LIMIT") {
        if (!fields.price) {
            errors.price = "Price is required for LIMIT orders";
        } else if (!isPositiveDecimal(fields.price)) {
            errors.price = "Price must be a positive decimal number";
        }
    }

    if (!fields.tif) {
        errors.time_in_force = "Time-in-force is required";
    }

    if (fields.tif === "GTD" && !fields.gtdDate) {
        errors.gtd_date = "Expiry date is required for GTD orders";
    }

    return errors;
}

// ---------------------------------------------------------------------------
// Payload builder  (exported for unit tests)
// ---------------------------------------------------------------------------

export function buildCreateOrderRequest(fields: {
    accountId: string;
    symbol: string;
    side: string;
    order_type: string;
    price: string;
    quantity: string;
    tif: string;
    gtdDate: string;
}): CreateOrderRequest {
    let time_in_force: TimeInForce;
    if (fields.tif === "GTD") {
        // Convert date string to Unix nanos
        const ms = new Date(fields.gtdDate).getTime();
        const nanos = String(BigInt(ms) * 1_000_000n);
        time_in_force = { type: "GTD", value: nanos };
    } else {
        time_in_force = { type: fields.tif } as TimeInForce;
    }

    return {
        account_id: fields.accountId,
        symbol: fields.symbol,
        side: fields.side as Side,
        order_type: fields.order_type as "LIMIT" | "MARKET",
        price: fields.order_type === "MARKET" ? "0" : fields.price,
        quantity: fields.quantity,
        time_in_force,
    };
}

// ---------------------------------------------------------------------------
// Submitted-order tracking (local state — not the authoritative store)
// ---------------------------------------------------------------------------

export interface SubmittedOrder {
    order_id: string;
    status: "PENDING" | "SYNCED";
    submitted_at: number; // Date.now()
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface OrderEntryProps {
    symbol: string;
    /** Optional account ID override. Defaults to "dev-account". */
    accountId?: string;
    /** Optional auth token override */
    token?: string;
}

const DEBOUNCE_MS = 500;

export const OrderEntry: React.FC<OrderEntryProps> = ({
    symbol,
    accountId = "dev-account",
    token = "dev-token-123",
}) => {
    // ---- form state --------------------------------------------------------
    const [side, setSide] = useState<string>("BUY");
    const [orderType, setOrderType] = useState<string>("LIMIT");
    const [price, setPrice] = useState<string>("");
    const [quantity, setQuantity] = useState<string>("");
    const [tif, setTif] = useState<string>("GTC");
    const [gtdDate, setGtdDate] = useState<string>("");

    // ---- submission state ---------------------------------------------------
    const [submitting, setSubmitting] = useState(false);
    const [errors, setErrors] = useState<ValidationErrors>({});
    const [submitError, setSubmitError] = useState<string | null>(null);
    const [submitSuccess, setSubmitSuccess] = useState<string | null>(null);
    const [rateLimited, setRateLimited] = useState(false);
    const [submittedOrders, setSubmittedOrders] = useState<SubmittedOrder[]>([]);
    const lastSubmitRef = useRef<number>(0);

    // ---- store access (for WS sync detection) ------------------------------
    const { store } = useDexStore();

    // ---- API client (singleton) --------------------------------------------
    const clientRef = useRef(
        new DexApiClient({ baseUrl: "/v1" }),
    );

    // ---- sync detection: mark orders SYNCED when store has them ------------
    useEffect(() => {
        const unsub = store.onStateChange((state) => {
            const acct = state.account;
            if (!acct) return;
            setSubmittedOrders((prev) =>
                prev.map((so) => {
                    if (so.status === "PENDING" && acct.orders[so.order_id]) {
                        return { ...so, status: "SYNCED" as const };
                    }
                    return so;
                }),
            );
        });
        return unsub;
    }, [store]);

    // ---- handlers ----------------------------------------------------------

    const handleReset = useCallback(() => {
        setSide("BUY");
        setOrderType("LIMIT");
        setPrice("");
        setQuantity("");
        setTif("GTC");
        setGtdDate("");
        setErrors({});
        setSubmitError(null);
        setSubmitSuccess(null);
    }, []);

    const handleSubmit = useCallback(
        async (e: React.FormEvent) => {
            e.preventDefault();

            // Client-side debounce
            const now = Date.now();
            if (now - lastSubmitRef.current < DEBOUNCE_MS) return;
            lastSubmitRef.current = now;

            // Validate
            const validationErrors = validateOrder({
                side,
                order_type: orderType,
                price,
                quantity,
                tif,
                gtdDate,
            });

            if (Object.keys(validationErrors).length > 0) {
                setErrors(validationErrors);
                return;
            }

            setErrors({});
            setSubmitError(null);
            setSubmitSuccess(null);
            setSubmitting(true);

            try {
                const req = buildCreateOrderRequest({
                    accountId,
                    symbol,
                    side,
                    order_type: orderType,
                    price,
                    quantity,
                    tif,
                    gtdDate,
                });

                const resp: OrderResponse =
                    await clientRef.current.createOrder(req, token);

                setSubmitSuccess(
                    `Order submitted (${resp.order_id}) — status: ${resp.status}`,
                );

                // Track locally
                setSubmittedOrders((prev) => [
                    { order_id: resp.order_id, status: "PENDING", submitted_at: Date.now() },
                    ...prev,
                ]);
            } catch (err: unknown) {
                if (err instanceof ApiError) {
                    if (err.status === 429) {
                        setRateLimited(true);
                        setSubmitError(
                            "Rate limited — please wait a moment before submitting again.",
                        );
                        setTimeout(() => setRateLimited(false), 5000);
                    } else {
                        const detail =
                            err.body?.message ?? err.body?.error ?? `HTTP ${err.status}`;
                        setSubmitError(`Order rejected: ${detail}`);
                    }
                } else {
                    setSubmitError(
                        `Unexpected error: ${err instanceof Error ? err.message : String(err)}`,
                    );
                }
            } finally {
                setSubmitting(false);
            }
        },
        [side, orderType, price, quantity, tif, gtdDate, accountId, symbol, token],
    );

    // ---- render ------------------------------------------------------------

    const isSubmitDisabled = submitting || rateLimited;

    return (
        <div id="order-entry" className="glass-panel p-6 rounded-2xl min-w-[320px] flex flex-col gap-5 text-slate-200 shadow-2xl relative overflow-hidden border-t border-indigo-500/20">
            {/* Background subtle glow */}
            <div className="absolute -top-24 -right-24 w-48 h-48 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none" />

            <h3 className="text-xl font-display font-bold tracking-tight text-white m-0 flex items-baseline gap-2">
                New Order
                <span className="text-slate-500 font-sans font-normal text-sm">— {symbol}</span>
            </h3>

            <form onSubmit={handleSubmit} id="order-entry-form" className="flex flex-col gap-3 relative z-10">
                <div className="grid grid-cols-2 gap-3">
                    <FieldRow label="Side" error={errors.side}>
                        <select
                            id="order-side"
                            value={side}
                            onChange={(e) => setSide(e.target.value)}
                            className="w-full px-3 py-2 bg-slate-900/60 border border-indigo-500/20 rounded-lg text-slate-200 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 transition-all cursor-pointer font-semibold"
                        >
                            <option value="BUY">BUY</option>
                            <option value="SELL">SELL</option>
                        </select>
                    </FieldRow>

                    <FieldRow label="Type" error={errors.order_type}>
                        <select
                            id="order-type"
                            value={orderType}
                            onChange={(e) => setOrderType(e.target.value)}
                            className="w-full px-3 py-2 bg-slate-900/60 border border-indigo-500/20 rounded-lg text-slate-200 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 transition-all cursor-pointer font-semibold"
                        >
                            <option value="LIMIT">LIMIT</option>
                            <option value="MARKET">MARKET</option>
                        </select>
                    </FieldRow>
                </div>

                {orderType === "LIMIT" && (
                    <FieldRow label="Price" error={errors.price}>
                        <div className="relative">
                            <input
                                id="order-price"
                                type="text"
                                inputMode="decimal"
                                placeholder="0.00"
                                value={price}
                                onChange={(e) => setPrice(e.target.value)}
                                className="w-full pl-3 pr-12 py-2 bg-slate-900/60 border border-indigo-500/20 rounded-lg text-slate-200 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 transition-all font-mono placeholder:text-slate-600 shadow-inner"
                            />
                            <div className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-slate-500 font-bold">USDT</div>
                        </div>
                    </FieldRow>
                )}

                <FieldRow label="Quantity" error={errors.quantity}>
                    <div className="relative">
                        <input
                            id="order-quantity"
                            type="text"
                            inputMode="decimal"
                            placeholder="0.00"
                            value={quantity}
                            onChange={(e) => setQuantity(e.target.value)}
                            className="w-full pl-3 pr-12 py-2 bg-slate-900/60 border border-indigo-500/20 rounded-lg text-slate-200 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 transition-all font-mono placeholder:text-slate-600 shadow-inner"
                        />
                        <div className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-slate-500 font-bold">{symbol.split('/')[0]}</div>
                    </div>
                </FieldRow>

                <div className="grid grid-cols-2 gap-3">
                    <FieldRow label="Time in Force" error={errors.time_in_force}>
                        <select
                            id="order-tif"
                            value={tif}
                            onChange={(e) => setTif(e.target.value)}
                            className="w-full px-3 py-2 bg-slate-900/60 border border-indigo-500/20 rounded-lg text-slate-200 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 transition-all cursor-pointer"
                        >
                            <option value="GTC">GTC</option>
                            <option value="IOC">IOC</option>
                            <option value="FOK">FOK</option>
                            <option value="GTD">GTD</option>
                        </select>
                    </FieldRow>

                    {tif === "GTD" && (
                        <FieldRow label="Expiry" error={errors.gtd_date}>
                            <input
                                id="order-gtd-date"
                                type="datetime-local"
                                value={gtdDate}
                                onChange={(e) => setGtdDate(e.target.value)}
                                className="w-full px-3 py-2 bg-slate-900/60 border border-indigo-500/20 rounded-lg text-slate-200 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 transition-all placeholder:text-slate-600"
                            />
                        </FieldRow>
                    )}
                </div>

                {/* Buttons */}
                <div className="flex gap-3 mt-4">
                    <button
                        id="order-submit"
                        type="submit"
                        disabled={isSubmitDisabled}
                        className={`
                            flex-1 py-3 px-4 rounded-xl font-bold tracking-wide text-sm text-white shadow-lg transition-all
                            ${isSubmitDisabled ? "opacity-50 cursor-not-allowed filter grayscale" : "hover:-translate-y-0.5 hover:shadow-xl active:translate-y-0"}
                            ${side === "BUY" ? "bg-gradient-to-r from-emerald-500 to-emerald-400 shadow-emerald-500/20" : "bg-gradient-to-r from-rose-500 to-rose-400 shadow-rose-500/20"}
                        `}
                    >
                        {submitting ? "Submitting…" : rateLimited ? "Rate limited" : `CONFIRM ${side}`}
                    </button>
                    <button
                        id="order-reset"
                        type="button"
                        onClick={handleReset}
                        className="px-4 py-3 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-xl font-semibold text-sm transition-colors border border-slate-700 hover:text-white"
                    >
                        Reset
                    </button>
                </div>
            </form>

            <div className="relative z-10 flex flex-col gap-2 empty:hidden">
                {/* Success toast */}
                {submitSuccess && (
                    <div id="order-success" className="p-3 bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 rounded-lg text-sm animate-fade-in flex items-center gap-2">
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" /></svg>
                        {submitSuccess}
                    </div>
                )}

                {/* Error toast */}
                {submitError && (
                    <div id="order-error" className="p-3 bg-rose-500/10 border border-rose-500/30 text-rose-400 rounded-lg text-sm animate-fade-in flex items-center gap-2">
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" /></svg>
                        {submitError}
                    </div>
                )}
            </div>

            {/* Local submitted-orders status panel */}
            {submittedOrders.length > 0 && (
                <div className="mt-2 relative z-10 animate-fade-in">
                    <h4 className="text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2">
                        Recent Activity
                    </h4>
                    <div className="bg-slate-900/50 rounded-lg border border-indigo-500/10 overflow-hidden">
                        <table className="w-full text-left text-sm whitespace-nowrap">
                            <thead className="bg-slate-800/50 text-xs text-slate-500 uppercase">
                                <tr>
                                    <th className="px-4 py-2 font-medium">Order ID</th>
                                    <th className="px-4 py-2 font-medium">Status</th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-indigo-500/5">
                                {submittedOrders.slice(0, 5).map((so) => (
                                    <tr key={so.order_id} className="hover:bg-slate-800/30 transition-colors">
                                        <td className="px-4 py-2 font-mono text-slate-300">
                                            {so.order_id.slice(0, 8)}…
                                        </td>
                                        <td className="px-4 py-2 font-semibold text-xs tracking-wide">
                                            <span className={so.status === "SYNCED" ? "text-emerald-400" : "text-amber-400"}>
                                                {so.status}
                                            </span>
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                </div>
            )}
        </div>
    );
};

// ---------------------------------------------------------------------------
// FieldRow — label + input + optional error
// ---------------------------------------------------------------------------

const FieldRow: React.FC<{
    label: string;
    error?: string;
    children: React.ReactNode;
}> = ({ label, error, children }) => (
    <div className="flex flex-col gap-1">
        <label className="text-xs font-semibold text-slate-400 tracking-wide ml-1">
            {label}
        </label>
        {children}
        {error && (
            <span className="text-xs text-rose-400 ml-1 animate-fade-in">{error}</span>
        )}
    </div>
);
