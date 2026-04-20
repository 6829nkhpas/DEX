// ---------------------------------------------------------------------------
// StatusIndicator — reusable status dot with consistent styling
// ---------------------------------------------------------------------------
//
// Usage:
//   <StatusIndicator status="connected" />
//   <StatusIndicator status="error" label="Disconnected" />
//   <StatusIndicator status="loading" pulse />
// ---------------------------------------------------------------------------

import React from "react";

export type StatusType =
    | "connected"
    | "disconnected"
    | "loading"
    | "error"
    | "warning"
    | "success"
    | "info"
    | "idle";

interface StatusIndicatorProps {
    status: StatusType;
    /** Optional label rendered next to the dot */
    label?: string;
    /** Force pulse animation regardless of status */
    pulse?: boolean;
    /** Size variant */
    size?: "sm" | "md" | "lg";
    className?: string;
}

const STATUS_CONFIG: Record<StatusType, { dotClass: string; labelClass: string; autoPulse: boolean }> = {
    connected: {
        dotClass: "bg-emerald-500 shadow-[0_0_6px_rgba(34,197,94,0.7)]",
        labelClass: "text-emerald-400",
        autoPulse: false,
    },
    success: {
        dotClass: "bg-emerald-500 shadow-[0_0_6px_rgba(34,197,94,0.7)]",
        labelClass: "text-emerald-400",
        autoPulse: false,
    },
    warning: {
        dotClass: "bg-amber-400 shadow-[0_0_6px_rgba(245,158,11,0.6)]",
        labelClass: "text-amber-400",
        autoPulse: false,
    },
    loading: {
        dotClass: "bg-amber-400",
        labelClass: "text-amber-400",
        autoPulse: true,
    },
    error: {
        dotClass: "bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.6)]",
        labelClass: "text-red-400",
        autoPulse: false,
    },
    disconnected: {
        dotClass: "bg-slate-500",
        labelClass: "text-slate-400",
        autoPulse: false,
    },
    info: {
        dotClass: "bg-indigo-500 shadow-[0_0_6px_rgba(99,102,241,0.5)]",
        labelClass: "text-indigo-400",
        autoPulse: false,
    },
    idle: {
        dotClass: "bg-slate-600",
        labelClass: "text-slate-500",
        autoPulse: false,
    },
};

const SIZE_MAP = {
    sm: "w-1.5 h-1.5",
    md: "w-2 h-2",
    lg: "w-2.5 h-2.5",
} as const;

const LABEL_SIZE_MAP = {
    sm: "text-[10px]",
    md: "text-xs",
    lg: "text-sm",
} as const;

export const StatusIndicator: React.FC<StatusIndicatorProps> = ({
    status,
    label,
    pulse,
    size = "md",
    className = "",
}) => {
    const config = STATUS_CONFIG[status];
    const shouldPulse = pulse ?? config.autoPulse;

    return (
        <span className={`inline-flex items-center gap-1.5 ${className}`} data-status={status}>
            <span
                className={`${SIZE_MAP[size]} rounded-full ${config.dotClass} ${shouldPulse ? "animate-pulse" : ""} shrink-0`}
            />
            {label && (
                <span className={`${LABEL_SIZE_MAP[size]} font-medium ${config.labelClass}`}>
                    {label}
                </span>
            )}
        </span>
    );
};
