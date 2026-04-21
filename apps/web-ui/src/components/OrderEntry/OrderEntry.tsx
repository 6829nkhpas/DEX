// ---------------------------------------------------------------------------
// OrderEntry — order submission form with validation, REST integration,
//              local submitted-orders tracking, and rate-limit handling
// ---------------------------------------------------------------------------
// Phase 15: improved BUY/SELL segmented toggle, submit button feedback,
//           auth gate with connect CTA, form field validation styling,
//           and consistent glass-panel design.
// ---------------------------------------------------------------------------

import React, { useState, useCallback, useRef, useEffect } from "react";
import Decimal from "decimal.js";
import { DexApiClient } from "../../api/rest-client";
import { ApiError } from "../../api/types";
import type { CreateOrderRequest, OrderResponse } from "../../api/types";
import type { Side, TimeInForce } from "../../../../../types/generated-types";
import { useDexStore, useAppSelector } from "../../state/StoreProvider";
import { useAuth } from "../../auth/AuthProvider";
import { useWallet } from "../../wallet/WalletProvider";
import { EmptyState } from "../ui/EmptyState";

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
    accountId: accountIdProp,
    token: tokenProp,
}) => {
    // ---- auth state --------------------------------------------------------
    const { authStatus, session, signIn } = useAuth();
    const { connect, address } = useWallet();
    const isAuthenticated = authStatus === "authenticated";
    // Use session credentials when authenticated, fall back to props for tests
    const accountId = session?.accountId ?? accountIdProp ?? "dev-account";
    const token = session?.signature ?? tokenProp ?? "dev-token-123";
    // ---- form state --------------------------------------------------------
    const [side, setSide] = useState<string>("BUY");
    const [orderType, setOrderType] = useState<string>("LIMIT");
    const [price, setPrice] = useState<string>("");
    const [quantity, setQuantity] = useState<string>("");
    const [tif, setTif] = useState<string>("GTC");
    const [gtdDate, setGtdDate] = useState<string>("");

    // ---- submission state ---------------------------------------------------
    const [submitting, setSubmitting] = useState(false);
    const [submitState, setSubmitState] = useState<"idle" | "success" | "error">("idle");
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

    // ---- Clear submit state feedback after delay
    useEffect(() => {
        if (submitState !== "idle") {
            const timer = setTimeout(() => setSubmitState("idle"), 1500);
            return () => clearTimeout(timer);
        }
    }, [submitState]);

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
        setSubmitState("idle");
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
            setSubmitState("idle");

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
                setSubmitState("success");

                // Track locally
                setSubmittedOrders((prev) => [
                    { order_id: resp.order_id, status: "PENDING", submitted_at: Date.now() },
                    ...prev,
                ]);
            } catch (err: unknown) {
                setSubmitState("error");
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

    const isSubmitDisabled = submitting || rateLimited || !isAuthenticated;
    const isBuy = side === "BUY";

    // ---- auth gate ---------------------------------------------------------
    if (!isAuthenticated) {
        return (
            <div id="order-entry" className="glass-panel p-6 rounded-2xl min-w-[300px] flex flex-col gap-5 text-slate-200 shadow-2xl relative overflow-hidden border-t border-indigo-500/20" style={{ gridArea: "entry" }}>
                <div className="absolute -top-24 -right-24 w-48 h-48 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none" />
                <h3 className="panel-header">
                    New Order
                    <span className="text-slate-500 font-sans font-normal text-sm ml-1">— {symbol}</span>
                </h3>
                <EmptyState
                    icon="lock"
                    message={!address ? "Connect wallet to place orders" : "Sign in to place orders"}
                    action={
                        !address
                            ? { label: "Connect Wallet", onClick: () => connect().catch(() => { }) }
                            : authStatus !== "signing"
                                ? { label: "Sign In", onClick: () => signIn().catch(() => { }) }
                                : undefined
                    }
                />
            </div>
        );
    }

    return (
        <div id="order-entry" className="glass-panel p-6 rounded-2xl min-w-[300px] flex flex-col gap-4 text-slate-200 shadow-2xl relative overflow-hidden border-t border-indigo-500/20" style={{ gridArea: "entry" }}>
            {/* Background subtle glow */}
            <div className="absolute -top-24 -right-24 w-48 h-48 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none" />

            <h3 className="panel-header">
                New Order
                <span className="text-slate-500 font-sans font-normal text-sm ml-1">— {symbol}</span>
            </h3>

            <form onSubmit={handleSubmit} id="order-entry-form" className="flex flex-col gap-3 relative z-10">
                {/* BUY / SELL segmented toggle */}
                <div className="grid grid-cols-2 gap-0 rounded-xl overflow-hidden border border-indigo-500/20">
                    <button
                        type="button"
                        id="order-side-buy"
                        onClick={() => setSide("BUY")}
                        className={`py-2.5 text-sm font-bold tracking-wide transition-all ${isBuy
                            ? "bg-gradient-to-r from-emerald-500/90 to-emerald-400/90 text-white shadow-lg"
                            : "bg-slate-900/60 text-slate-400 hover:text-white hover:bg-slate-800/60"
                            }`}
                    >
                        BUY
                    </button>
                    <button
                        type="button"
                        id="order-side-sell"
                        onClick={() => setSide("SELL")}
                        className={`py-2.5 text-sm font-bold tracking-wide transition-all ${!isBuy
                            ? "bg-gradient-to-r from-rose-500/90 to-rose-400/90 text-white shadow-lg"
                            : "bg-slate-900/60 text-slate-400 hover:text-white hover:bg-slate-800/60"
                            }`}
                    >
                        SELL
                    </button>
                </div>

                {/* Order type */}
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
                                className={`w-full pl-3 pr-12 py-2 bg-slate-900/60 border rounded-lg text-slate-200 text-sm focus:outline-none focus:ring-2 transition-all font-mono placeholder:text-slate-600 shadow-inner ${errors.price ? "border-rose-500/40 focus:ring-rose-500/50" : "border-indigo-500/20 focus:ring-indigo-500/50"}`}
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
                            className={`w-full pl-3 pr-12 py-2 bg-slate-900/60 border rounded-lg text-slate-200 text-sm focus:outline-none focus:ring-2 transition-all font-mono placeholder:text-slate-600 shadow-inner ${errors.quantity ? "border-rose-500/40 focus:ring-rose-500/50" : "border-indigo-500/20 focus:ring-indigo-500/50"}`}
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
                <div className="flex gap-3 mt-3">
                    <button
                        id="order-submit"
                        type="submit"
                        disabled={isSubmitDisabled}
                        className={`
                            flex-1 py-3 px-4 rounded-xl font-bold tracking-wide text-sm text-white shadow-lg transition-all flex items-center justify-center gap-2
                            ${isSubmitDisabled ? "opacity-50 cursor-not-allowed filter grayscale" : "hover:-translate-y-0.5 hover:shadow-xl active:translate-y-0"}
                            ${submitState === "error" ? "animate-shake" : ""}
                            ${isBuy ? "bg-gradient-to-r from-emerald-500 to-emerald-400 shadow-emerald-500/20" : "bg-gradient-to-r from-rose-500 to-rose-400 shadow-rose-500/20"}
                        `}
                    >
                        {submitting ? (
                            <>
                                <svg className="animate-spin w-4 h-4" fill="none" viewBox="0 0 24 24">
                                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                                </svg>
                                Submitting…
                            </>
                        ) : submitState === "success" ? (
                            <>
                                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
                                </svg>
                                Submitted
                            </>
                        ) : rateLimited ? (
                            "Rate limited"
                        ) : (
                            `CONFIRM ${side}`
                        )}
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
                        <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" /></svg>
                        {submitSuccess}
                    </div>
                )}

                {/* Error toast */}
                {submitError && (
                    <div id="order-error" className="p-3 bg-rose-500/10 border border-rose-500/30 text-rose-400 rounded-lg text-sm animate-fade-in flex items-center gap-2">
                        <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" /></svg>
                        {submitError}
                    </div>
                )}
            </div>

            {/* Local submitted-orders status panel */}
            {submittedOrders.length > 0 && (
                <div className="mt-1 relative z-10 animate-fade-in">
                    <h4 className="text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2">
                        Recent Activity
                    </h4>
                    <div className="bg-slate-900/50 rounded-lg border border-indigo-500/10 overflow-hidden">
                        <table className="data-table">
                            <thead>
                                <tr>
                                    <th>Order ID</th>
                                    <th>Status</th>
                                </tr>
                            </thead>
                            <tbody>
                                {submittedOrders.slice(0, 5).map((so) => (
                                    <tr key={so.order_id}>
                                        <td className="font-mono text-slate-300">
                                            {so.order_id.slice(0, 8)}…
                                        </td>
                                        <td className="font-semibold text-xs tracking-wide">
                                            <span className={`status-badge ${so.status === "SYNCED" ? "status-badge-success" : "status-badge-warning"}`}>
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
            <span className="text-xs text-rose-400 ml-1 animate-fade-in flex items-center gap-1">
                <svg className="w-3 h-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01" />
                </svg>
                {error}
            </span>
        )}
    </div>
);
