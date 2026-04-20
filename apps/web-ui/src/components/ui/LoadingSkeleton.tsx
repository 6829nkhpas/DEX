// ---------------------------------------------------------------------------
// LoadingSkeleton — animated placeholder for loading states
// ---------------------------------------------------------------------------
//
// Usage:
//   <LoadingSkeleton variant="row" count={5} />
//   <LoadingSkeleton variant="card" />
//   <LoadingSkeleton variant="text" width="60%" />
// ---------------------------------------------------------------------------

import React from "react";

type SkeletonVariant = "row" | "card" | "text" | "ticker";

interface LoadingSkeletonProps {
    variant?: SkeletonVariant;
    /** Number of skeleton items to render */
    count?: number;
    /** Width override for text variant */
    width?: string;
    className?: string;
}

const SkeletonRow: React.FC = () => (
    <div className="flex justify-between items-center py-2 px-4">
        <div className="animate-skeleton h-3.5 w-20 rounded" />
        <div className="animate-skeleton h-3.5 w-14 rounded" />
        <div className="animate-skeleton h-3.5 w-16 rounded" />
    </div>
);

const SkeletonCard: React.FC = () => (
    <div className="p-5 space-y-3">
        <div className="animate-skeleton h-4 w-24 rounded" />
        <div className="animate-skeleton h-3 w-full rounded" />
        <div className="animate-skeleton h-3 w-3/4 rounded" />
        <div className="flex gap-2 pt-1">
            <div className="animate-skeleton h-8 flex-1 rounded-lg" />
            <div className="animate-skeleton h-8 flex-1 rounded-lg" />
        </div>
    </div>
);

const SkeletonText: React.FC<{ width?: string }> = ({ width = "100%" }) => (
    <div className="animate-skeleton h-3.5 rounded" style={{ width }} />
);

const SkeletonTicker: React.FC = () => (
    <div className="flex items-center gap-6 p-4">
        <div className="animate-skeleton h-6 w-24 rounded" />
        <div className="flex flex-col gap-1.5">
            <div className="animate-skeleton h-2.5 w-16 rounded" />
            <div className="animate-skeleton h-5 w-28 rounded" />
        </div>
        <div className="flex gap-6">
            {[...Array(4)].map((_, i) => (
                <div key={i} className="flex flex-col gap-1.5">
                    <div className="animate-skeleton h-2.5 w-12 rounded" />
                    <div className="animate-skeleton h-3.5 w-16 rounded" />
                </div>
            ))}
        </div>
    </div>
);

export const LoadingSkeleton: React.FC<LoadingSkeletonProps> = ({
    variant = "row",
    count = 1,
    width,
    className = "",
}) => {
    const items = Array.from({ length: count }, (_, i) => i);

    return (
        <div className={`${className}`} data-testid="loading-skeleton" data-variant={variant}>
            {variant === "card" && <SkeletonCard />}
            {variant === "ticker" && <SkeletonTicker />}
            {variant === "row" &&
                items.map((i) => <SkeletonRow key={i} />)}
            {variant === "text" &&
                items.map((i) => (
                    <SkeletonText key={i} width={width} />
                ))}
        </div>
    );
};
