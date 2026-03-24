// ---------------------------------------------------------------------------
// AuthGate.tsx — render children only when authenticated
// ---------------------------------------------------------------------------
//
// Usage:
//   <AuthGate>
//     <PlaceOrderButton />
//   </AuthGate>
//
//   <AuthGate fallback={<p>Sign in to trade</p>}>
//     <CancelOrderButton />
//   </AuthGate>
// ---------------------------------------------------------------------------

import React from "react";
import { useAuth } from "./AuthProvider";

export interface AuthGateProps {
    children: React.ReactNode;
    /**
     * Content to render when NOT authenticated.
     * Defaults to a generic inline "Sign in to access" message.
     */
    fallback?: React.ReactNode;
}

const DefaultFallback: React.FC = () => (
    <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-400 text-xs font-medium">
        <svg className="w-3.5 h-3.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
        </svg>
        Sign in to access this feature
    </div>
);

export const AuthGate: React.FC<AuthGateProps> = ({
    children,
    fallback = <DefaultFallback />,
}) => {
    const { authStatus } = useAuth();

    if (authStatus === "authenticated") {
        return <>{children}</>;
    }

    return <>{fallback}</>;
};
