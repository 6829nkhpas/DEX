// ---------------------------------------------------------------------------
// EmptyState — standardized empty/placeholder state for panels
// ---------------------------------------------------------------------------
//
// Usage:
//   <EmptyState message="No open orders" />
//   <EmptyState message="Sign in to view" icon="lock" action={{ label: "Sign In", onClick: fn }} />
// ---------------------------------------------------------------------------

import React from "react";

export type EmptyIcon = "empty" | "lock" | "chart" | "wallet" | "search";

interface EmptyStateAction {
    label: string;
    onClick: () => void;
}

interface EmptyStateProps {
    message: string;
    /** Optional sub-message for more context */
    detail?: string;
    icon?: EmptyIcon;
    action?: EmptyStateAction;
    className?: string;
}

const ICON_PATHS: Record<EmptyIcon, string> = {
    empty: "M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4",
    lock: "M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z",
    chart: "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z",
    wallet: "M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z",
    search: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z",
};

export const EmptyState: React.FC<EmptyStateProps> = ({
    message,
    detail,
    icon = "empty",
    action,
    className = "",
}) => {
    return (
        <div
            className={`flex flex-col items-center justify-center py-8 px-4 text-center bg-slate-900/30 rounded-xl border border-indigo-500/10 ${className}`}
            data-testid="empty-state"
        >
            <svg
                className="w-8 h-8 text-slate-600 mb-3"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                aria-hidden="true"
            >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d={ICON_PATHS[icon]} />
            </svg>
            <span className="text-sm font-medium text-slate-500">{message}</span>
            {detail && (
                <span className="text-xs text-slate-600 mt-1">{detail}</span>
            )}
            {action && (
                <button
                    onClick={action.onClick}
                    className="mt-3 text-xs font-bold px-4 py-1.5 rounded-lg bg-indigo-500/15 border border-indigo-500/25 text-indigo-400 hover:bg-indigo-500 hover:text-white hover:border-indigo-500 transition-all"
                >
                    {action.label}
                </button>
            )}
        </div>
    );
};
