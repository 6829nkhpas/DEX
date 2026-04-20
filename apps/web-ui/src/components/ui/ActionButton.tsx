// ---------------------------------------------------------------------------
// ActionButton — stateful button with built-in pending/success/error feedback
// ---------------------------------------------------------------------------
//
// Usage:
//   <ActionButton
//     variant="buy"
//     label="CONFIRM BUY"
//     pendingLabel="Submitting..."
//     onClick={handleSubmit}
//     disabled={!isAuthenticated}
//   />
// ---------------------------------------------------------------------------

import React, { useState, useCallback, useRef, useEffect } from "react";

export type ButtonVariant = "primary" | "buy" | "sell" | "danger" | "ghost";
export type ButtonState = "idle" | "pending" | "success" | "error" | "disabled";

interface ActionButtonProps {
    variant?: ButtonVariant;
    label: string;
    pendingLabel?: string;
    successLabel?: string;
    errorLabel?: string;
    onClick?: () => void | Promise<void>;
    disabled?: boolean;
    type?: "button" | "submit";
    /** If true, manages state internally based on onClick promise */
    autoManage?: boolean;
    /** External state override (takes priority over internal state) */
    state?: ButtonState;
    /** Duration to show success/error state before returning to idle (ms) */
    feedbackDuration?: number;
    className?: string;
    id?: string;
}

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
    primary: "btn-action btn-action-primary",
    buy: "btn-action btn-action-buy",
    sell: "btn-action btn-action-sell",
    danger: "btn-action btn-action-danger",
    ghost: "btn-action btn-action-ghost",
};

const Spinner: React.FC = () => (
    <svg className="animate-spin w-4 h-4" fill="none" viewBox="0 0 24 24">
        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
    </svg>
);

const CheckIcon: React.FC = () => (
    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M5 13l4 4L19 7" />
    </svg>
);

const XIcon: React.FC = () => (
    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 18L18 6M6 6l12 12" />
    </svg>
);

export const ActionButton: React.FC<ActionButtonProps> = ({
    variant = "primary",
    label,
    pendingLabel,
    successLabel,
    errorLabel,
    onClick,
    disabled = false,
    type = "button",
    autoManage = false,
    state: externalState,
    feedbackDuration = 1500,
    className = "",
    id,
}) => {
    const [internalState, setInternalState] = useState<ButtonState>("idle");
    const feedbackTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

    const currentState = externalState ?? (disabled ? "disabled" : internalState);

    // Cleanup timer on unmount
    useEffect(() => {
        return () => {
            if (feedbackTimer.current) clearTimeout(feedbackTimer.current);
        };
    }, []);

    const handleClick = useCallback(async () => {
        if (currentState === "pending" || currentState === "disabled") return;
        if (!onClick) return;

        if (!autoManage) {
            onClick();
            return;
        }

        setInternalState("pending");
        try {
            await onClick();
            setInternalState("success");
            feedbackTimer.current = setTimeout(() => setInternalState("idle"), feedbackDuration);
        } catch {
            setInternalState("error");
            feedbackTimer.current = setTimeout(() => setInternalState("idle"), feedbackDuration);
        }
    }, [onClick, autoManage, currentState, feedbackDuration]);

    const displayLabel = (() => {
        switch (currentState) {
            case "pending":
                return pendingLabel ?? "Processing…";
            case "success":
                return successLabel ?? "Done";
            case "error":
                return errorLabel ?? "Failed";
            default:
                return label;
        }
    })();

    const stateIcon = (() => {
        switch (currentState) {
            case "pending":
                return <Spinner />;
            case "success":
                return <CheckIcon />;
            case "error":
                return <XIcon />;
            default:
                return null;
        }
    })();

    const shakeClass = currentState === "error" ? "animate-shake" : "";

    return (
        <button
            id={id}
            type={type}
            disabled={currentState === "disabled" || currentState === "pending"}
            onClick={handleClick}
            className={`${VARIANT_CLASSES[variant]} ${shakeClass} ${className}`}
            data-state={currentState}
        >
            {stateIcon}
            {displayLabel}
        </button>
    );
};
