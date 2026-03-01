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
        <div
            id="order-entry"
            style={{
                background: "#111827",
                border: "1px solid #374151",
                borderRadius: 8,
                padding: 16,
                minWidth: 300,
                fontFamily: "Inter, system-ui, sans-serif",
                color: "#e5e7eb",
            }}
        >
            <h3 style={{ margin: "0 0 12px", fontSize: 16, fontWeight: 600 }}>
                New Order — {symbol}
            </h3>

            <form onSubmit={handleSubmit} id="order-entry-form">
                {/* Side */}
                <FieldRow label="Side" error={errors.side}>
                    <select
                        id="order-side"
                        value={side}
                        onChange={(e) => setSide(e.target.value)}
                        style={inputStyle}
                    >
                        <option value="BUY">BUY</option>
                        <option value="SELL">SELL</option>
                    </select>
                </FieldRow>

                {/* Type */}
                <FieldRow label="Type" error={errors.order_type}>
                    <select
                        id="order-type"
                        value={orderType}
                        onChange={(e) => setOrderType(e.target.value)}
                        style={inputStyle}
                    >
                        <option value="LIMIT">LIMIT</option>
                        <option value="MARKET">MARKET</option>
                    </select>
                </FieldRow>

                {/* Price (hidden for MARKET) */}
                {orderType === "LIMIT" && (
                    <FieldRow label="Price" error={errors.price}>
                        <input
                            id="order-price"
                            type="text"
                            inputMode="decimal"
                            placeholder="e.g. 50000.00"
                            value={price}
                            onChange={(e) => setPrice(e.target.value)}
                            style={inputStyle}
                        />
                    </FieldRow>
                )}

                {/* Quantity */}
                <FieldRow label="Quantity" error={errors.quantity}>
                    <input
                        id="order-quantity"
                        type="text"
                        inputMode="decimal"
                        placeholder="e.g. 1.0"
                        value={quantity}
                        onChange={(e) => setQuantity(e.target.value)}
                        style={inputStyle}
                    />
                </FieldRow>

                {/* TIF */}
                <FieldRow label="Time in Force" error={errors.time_in_force}>
                    <select
                        id="order-tif"
                        value={tif}
                        onChange={(e) => setTif(e.target.value)}
                        style={inputStyle}
                    >
                        <option value="GTC">GTC</option>
                        <option value="IOC">IOC</option>
                        <option value="FOK">FOK</option>
                        <option value="GTD">GTD</option>
                    </select>
                </FieldRow>

                {/* GTD date */}
                {tif === "GTD" && (
                    <FieldRow label="Expiry" error={errors.gtd_date}>
                        <input
                            id="order-gtd-date"
                            type="datetime-local"
                            value={gtdDate}
                            onChange={(e) => setGtdDate(e.target.value)}
                            style={inputStyle}
                        />
                    </FieldRow>
                )}

                {/* Buttons */}
                <div style={{ display: "flex", gap: 8, marginTop: 12 }}>
                    <button
                        id="order-submit"
                        type="submit"
                        disabled={isSubmitDisabled}
                        style={{
                            ...btnStyle,
                            background: side === "BUY" ? "#10b981" : "#ef4444",
                            opacity: isSubmitDisabled ? 0.5 : 1,
                            cursor: isSubmitDisabled ? "not-allowed" : "pointer",
                        }}
                    >
                        {submitting ? "⏳ Submitting…" : rateLimited ? "⏳ Rate limited" : `Submit ${side}`}
                    </button>
                    <button
                        id="order-reset"
                        type="button"
                        onClick={handleReset}
                        style={{ ...btnStyle, background: "#374151" }}
                    >
                        Reset
                    </button>
                </div>
            </form>

            {/* Success toast */}
            {submitSuccess && (
                <div
                    id="order-success"
                    style={{
                        marginTop: 8,
                        padding: 8,
                        background: "#065f46",
                        borderRadius: 4,
                        fontSize: 13,
                    }}
                >
                    ✅ {submitSuccess}
                </div>
            )}

            {/* Error toast */}
            {submitError && (
                <div
                    id="order-error"
                    style={{
                        marginTop: 8,
                        padding: 8,
                        background: "#7f1d1d",
                        borderRadius: 4,
                        fontSize: 13,
                    }}
                >
                    ❌ {submitError}
                </div>
            )}

            {/* Local submitted-orders status panel */}
            {submittedOrders.length > 0 && (
                <div style={{ marginTop: 12 }}>
                    <h4 style={{ fontSize: 13, fontWeight: 600, marginBottom: 4 }}>
                        Recent Submissions
                    </h4>
                    <table style={{ width: "100%", fontSize: 12, borderCollapse: "collapse" }}>
                        <thead>
                            <tr style={{ borderBottom: "1px solid #374151" }}>
                                <th style={thStyle}>Order ID</th>
                                <th style={thStyle}>Status</th>
                            </tr>
                        </thead>
                        <tbody>
                            {submittedOrders.slice(0, 10).map((so) => (
                                <tr key={so.order_id}>
                                    <td style={tdStyle}>
                                        {so.order_id.slice(0, 8)}…
                                    </td>
                                    <td style={tdStyle}>
                                        <span
                                            style={{
                                                color:
                                                    so.status === "SYNCED"
                                                        ? "#10b981"
                                                        : "#fbbf24",
                                                fontWeight: 600,
                                            }}
                                        >
                                            {so.status}
                                        </span>
                                    </td>
                                </tr>
                            ))}
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

const inputStyle: React.CSSProperties = {
    width: "100%",
    padding: "6px 8px",
    background: "#1f2937",
    border: "1px solid #4b5563",
    borderRadius: 4,
    color: "#e5e7eb",
    fontSize: 13,
    boxSizing: "border-box",
};

const btnStyle: React.CSSProperties = {
    flex: 1,
    padding: "8px 0",
    border: "none",
    borderRadius: 4,
    color: "#fff",
    fontWeight: 600,
    fontSize: 13,
};

const thStyle: React.CSSProperties = {
    textAlign: "left",
    padding: "4px 6px",
    color: "#9ca3af",
};

const tdStyle: React.CSSProperties = {
    padding: "4px 6px",
};

// ---------------------------------------------------------------------------
// FieldRow — label + input + optional error
// ---------------------------------------------------------------------------

const FieldRow: React.FC<{
    label: string;
    error?: string;
    children: React.ReactNode;
}> = ({ label, error, children }) => (
    <div style={{ marginBottom: 8 }}>
        <label
            style={{
                display: "block",
                fontSize: 12,
                color: "#9ca3af",
                marginBottom: 2,
            }}
        >
            {label}
        </label>
        {children}
        {error && (
            <div
                className="field-error"
                style={{ color: "#f87171", fontSize: 11, marginTop: 2 }}
            >
                {error}
            </div>
        )}
    </div>
);
