// ---------------------------------------------------------------------------
// GovernanceContext.tsx — admin role and governance audit trail
// ---------------------------------------------------------------------------
//
// Provides:
//   GovernanceProvider  — wraps app children; derives adminRole from auth session
//   GovernanceGuard     — renders children only if user has sufficient role
//   useGovernance()     — hook exposing adminRole + logAction
//   useGovernanceAudit()— hook exposing in-memory audit log (readonly)
//
// Role hierarchy (descending authority):
//   "super" > "risk" > "support" > "none"
//
// In v1 the role is derived from a dev-mode static allowlist keyed by accountId.
// In a future release this will be replaced by a backend-issued role claim in
// the auth token. The interface is stable.
//
// SECURITY NOTE:
//   Frontend role checks are UX-only convenience gates. All sensitive backend
//   operations enforce role checks server-side. Never trust frontend-only gating.
// ---------------------------------------------------------------------------

import React, {
    createContext,
    useContext,
    useState,
    useCallback,
    useEffect,
    useMemo,
    useRef,
} from "react";
import { useAuth } from "./AuthProvider";
import { logger } from "../infra/logger";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type AdminRole = "none" | "support" | "risk" | "super";

export interface GovernanceAuditEntry {
    timestamp: string;
    action: string;
    accountId: string;
    role: AdminRole;
}

export interface GovernanceContextValue {
    /** Current derived admin role for the authenticated session. */
    adminRole: AdminRole;
    /**
     * Log a governance action to the in-memory audit trail and telemetry.
     * Should be called before executing any sensitive admin operation.
     */
    logAction: (action: string) => void;
}

export interface GovernanceAuditContextValue {
    /** Readonly snapshot of the in-memory governance audit log. */
    auditLog: readonly GovernanceAuditEntry[];
}

// ---------------------------------------------------------------------------
// Dev-mode static role allowlist
// Keyed by accountId (derived). In production, replace with backend claim.
// ---------------------------------------------------------------------------

const DEV_ROLE_ALLOWLIST: Record<string, AdminRole> = {
    // Example: set these to the accountIds derived from your dev wallet addresses.
    // "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx": "super",
};

function deriveRole(accountId: string | null): AdminRole {
    if (!accountId) return "none";
    return DEV_ROLE_ALLOWLIST[accountId] ?? "none";
}

// ---------------------------------------------------------------------------
// Role hierarchy helper
// ---------------------------------------------------------------------------

const ROLE_ORDER: Record<AdminRole, number> = {
    none: 0,
    support: 1,
    risk: 2,
    super: 3,
};

/** Returns true if `actual` meets or exceeds the `required` role level. */
export function hasRole(actual: AdminRole, required: AdminRole): boolean {
    return ROLE_ORDER[actual] >= ROLE_ORDER[required];
}

// ---------------------------------------------------------------------------
// Contexts
// ---------------------------------------------------------------------------

const GovernanceCtx = createContext<GovernanceContextValue | null>(null);
const GovernanceAuditCtx = createContext<GovernanceAuditContextValue | null>(null);

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export function GovernanceProvider({ children }: { children: React.ReactNode }) {
    const { authStatus, session } = useAuth();
    const [adminRole, setAdminRole] = useState<AdminRole>("none");
    const auditLogRef = useRef<GovernanceAuditEntry[]>([]);
    // We keep a state version counter so consumers re-render on log changes
    const [, setAuditVersion] = useState(0);

    // Derive role whenever auth session changes
    useEffect(() => {
        if (authStatus !== "authenticated" || !session) {
            setAdminRole("none");
            return;
        }
        const role = deriveRole(session.accountId);
        setAdminRole(role);
        if (role !== "none") {
            logger.info("GovernanceProvider: admin role derived", {
                accountId: session.accountId,
                role,
            });
        }
    }, [authStatus, session]);

    const logAction = useCallback(
        (action: string) => {
            const entry: GovernanceAuditEntry = {
                timestamp: new Date().toISOString(),
                action,
                accountId: session?.accountId ?? "anonymous",
                role: adminRole,
            };
            auditLogRef.current = [...auditLogRef.current, entry];
            setAuditVersion((v) => v + 1);
            logger.info("GovernanceAudit: action logged", {
                action,
                role: adminRole,
                accountId: entry.accountId,
            });
        },
        [adminRole, session],
    );

    const governanceValue = useMemo<GovernanceContextValue>(
        () => ({ adminRole, logAction }),
        [adminRole, logAction],
    );

    const auditValue = useMemo<GovernanceAuditContextValue>(
        () => ({ auditLog: auditLogRef.current }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [auditLogRef.current.length],
    );

    return (
        <GovernanceCtx.Provider value={governanceValue}>
            <GovernanceAuditCtx.Provider value={auditValue}>
                {children}
            </GovernanceAuditCtx.Provider>
        </GovernanceCtx.Provider>
    );
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

export function useGovernance(): GovernanceContextValue {
    const ctx = useContext(GovernanceCtx);
    if (!ctx) {
        throw new Error("useGovernance must be used within a <GovernanceProvider>");
    }
    return ctx;
}

export function useGovernanceAudit(): GovernanceAuditContextValue {
    const ctx = useContext(GovernanceAuditCtx);
    if (!ctx) {
        throw new Error("useGovernanceAudit must be used within a <GovernanceProvider>");
    }
    return ctx;
}

// ---------------------------------------------------------------------------
// GovernanceGuard
// ---------------------------------------------------------------------------

export interface GovernanceGuardProps {
    /** Minimum role required to render children. */
    requiredRole: AdminRole;
    children: React.ReactNode;
    /**
     * What to render when the user's role is insufficient.
     * Defaults to null (renders nothing).
     */
    fallback?: React.ReactNode;
}

/**
 * Renders children only when the current user's adminRole meets the
 * requiredRole threshold. Otherwise renders fallback (default: null).
 *
 * Example:
 *   <GovernanceGuard requiredRole="risk">
 *     <ForceCloseButton />
 *   </GovernanceGuard>
 */
export const GovernanceGuard: React.FC<GovernanceGuardProps> = ({
    requiredRole,
    children,
    fallback = null,
}) => {
    const { adminRole } = useGovernance();

    if (hasRole(adminRole, requiredRole)) {
        return <>{children}</>;
    }

    return <>{fallback}</>;
};
